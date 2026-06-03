use api::ToolDefinition;
use async_trait::async_trait;
use medical_core::types::CitationStyle;
use serde_json::{json, Value};

use super::{Tool, ToolContext};
use crate::backend::ChatEvent;

// ---------------------------------------------------------------------------
// search_pubmed
// ---------------------------------------------------------------------------

pub struct SearchPubmed;

#[async_trait]
impl Tool for SearchPubmed {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search_pubmed".into(),
            description: Some(
                "Search PubMed for medical literature. Call this when the user asks about \
                 any medical topic, disease, drug, gene, or wants to find research papers. \
                 Returns a list of papers with PMID, title, authors, journal, and year."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "PubMed search query. Use MeSH terms and Boolean operators (AND, OR, NOT) for precision."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum results (1-20, default 10)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let query = input["query"]
            .as_str()
            .ok_or("Missing 'query' parameter")?;
        let limit = input["max_results"].as_u64().unwrap_or(10) as u32;
        let papers = ctx
            .medical
            .search_pubmed(query, limit)
            .await
            .map_err(|e| format!("PubMed search error: {e}"))?;

        // Emit structured search results to the UI
        ctx.send_event(ChatEvent::SearchResults(papers.clone()));

        if papers.is_empty() {
            Ok("No results found. Try broader search terms.".into())
        } else {
            let summary: Vec<String> = papers
                .iter()
                .map(|p| {
                    let authors = if p.authors.is_empty() {
                        "Unknown".to_string()
                    } else if p.authors.len() == 1 {
                        p.authors[0].to_string()
                    } else {
                        format!("{} et al.", p.authors[0])
                    };
                    let journal = p.journal.as_deref().unwrap_or("Unknown Journal");
                    let year = p.year.as_deref().unwrap_or("n.d.");
                    let title = &p.title;
                    format!(
                        "PMID:{}\n  {}\n  {} — {} ({})\n",
                        p.pmid, title, authors, journal, year
                    )
                })
                .collect();
            Ok(format!(
                "Found {} results:\n\n{}",
                papers.len(),
                summary.join("\n")
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// fetch_article
// ---------------------------------------------------------------------------

pub struct FetchArticle;

#[async_trait]
impl Tool for FetchArticle {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "fetch_article".into(),
            description: Some(
                "Fetch detailed metadata for a specific PubMed article by PMID. \
                 Returns title, abstract, authors, journal, year, DOI, etc."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pmid": {
                        "type": "string",
                        "description": "PubMed ID (PMID) of the article"
                    }
                },
                "required": ["pmid"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let pmid = input["pmid"]
            .as_str()
            .ok_or("Missing 'pmid' parameter")?;
        let paper = ctx
            .medical
            .fetch_article(pmid)
            .await
            .map_err(|e| format!("PubMed fetch error: {e}"))?;

        match paper {
            None => Ok(format!("No article found for PMID: {pmid}")),
            Some(p) => {
                let abstract_text = p
                    .abstract_text
                    .as_deref()
                    .unwrap_or("No abstract available");
                Ok(format!(
                    "Title: {}\nAuthors: {}\nJournal: {}\nYear: {}\nDOI: {}\n\nAbstract:\n{}",
                    p.title,
                    p.authors
                        .iter()
                        .map(|a| a.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    p.journal.as_deref().unwrap_or("N/A"),
                    p.year.as_deref().unwrap_or("N/A"),
                    p.doi.as_deref().unwrap_or("N/A"),
                    abstract_text,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// format_citation
// ---------------------------------------------------------------------------

pub struct FormatCitation;

#[async_trait]
impl Tool for FormatCitation {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "format_citation".into(),
            description: Some(
                "Format one or more papers into a specific citation style. \
                 Use this when the user asks to format references."
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pmids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "List of PubMed IDs to format"
                    },
                    "style": {
                        "type": "string",
                        "enum": ["apa", "vancouver", "bibtex", "ris", "mla"],
                        "description": "Citation style"
                    }
                },
                "required": ["pmids", "style"]
            }),
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, String> {
        let pmids: Vec<String> = input["pmids"]
            .as_array()
            .ok_or("Missing 'pmids' parameter")?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let style_str = input["style"]
            .as_str()
            .ok_or("Missing 'style' parameter")?;
        let style = CitationStyle::from_str(style_str)
            .ok_or(format!("Unknown citation style: {style_str}"))?;

        let papers = ctx
            .medical
            .pubmed
            .fetch_articles(&pmids)
            .await
            .map_err(|e| format!("PubMed fetch error: {e}"))?;

        let formatted = ctx.medical.format_citations(&papers, style);
        Ok(formatted)
    }
}
