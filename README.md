# 🏛️ Galen

<p align="center">
  <strong>医学科研助手 — 搜文献、读论文、写文章，一站式搞定</strong>
</p>

<p align="center">
  <a href="https://github.com/Drehabwen/Galen">GitHub</a>
  ·
  <a href="#快速开始">快速开始</a>
  ·
  <a href="#功能">功能</a>
  ·
  <a href="#模型配置">模型配置</a>
</p>

---

Galen 是一个**原生桌面应用**，专为医学生和科研人员设计。双击 exe 即可启动，无需终端、无需编程。命名致敬古希腊医学之父盖伦（Galen of Pergamon），寓意用最前沿的 AI 技术延续最古老的医学求真传统。

**核心理念：** 把你留在应用里——搜文献、读论文、写文章、格式化引用，一站式完成。

## 功能

| 模块 | 说明 |
|------|------|
| 🤖 **多模型聊天** | 支持 Claude / GPT / DeepSeek / 本地模型，流式对话，零门槛配置 |
| 📚 **文献检索** | PubMed 搜索，一键加载摘要，MeSH 术语查询 |
| 📝 **引用格式化** | 支持 APA、Vancouver、BibTeX、RIS、MLA 五种格式 |
| 📄 **工作区** | 左栏查看论文、编辑文档、预览模板 |
| 🎨 **暗色主题** | 护眼深色界面，中文字体原生支持 |

## 快速开始

### Windows 用户（推荐）

1. 从 [Releases](../../releases) 下载 `galen.exe`
2. 双击运行，窗口自动打开
3. 选模型、开聊

### 从源码构建

```bash
git clone https://github.com/Drehabwen/Galen
cd Galen/rust
cargo build --release -p galen
# exe 在 target/release/galen.exe
```

## 模型配置

在 `%USERPROFILE%\.claw\models.toml` 中配置你的模型：

```toml
[router]
default = "sonnet"

[models.sonnet]
provider = "anthropic"
api_key = "sk-ant-xxx"
model_id = "claude-sonnet-4-6"

[models.gpt4o]
provider = "openai"
api_key = "sk-xxx"
model_id = "gpt-4o"
```

没有配置文件时，应用会自动检测环境变量中的 API key 并使用内置默认模型列表。

## 架构

```
api/              ── 多 Provider LLM 调用（Anthropic / OpenAI / DeepSeek / 兼容）
model-router/     ── 模型配置抽象，TOML → ProviderClient
medical-core/     ── PubMed/MeSH 客户端，引用格式化引擎
galen/            ── Tauri 2.x 桌面应用，React + TypeScript 前端
runtime/          ── 对话运行时 & 工具执行
plugins/          ── 插件系统 & MCP 集成
tools/            ── 工具注册与执行
```

## 许可

MIT

---

<p align="center">
  Made for medical researchers who just want to get work done.
</p>
