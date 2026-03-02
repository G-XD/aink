# aink

**追踪与分析 AI 编程工具使用情况。**

[English](README.md)

AINK 是一款终端 TUI，用于发现、加载并分析 AI 编程会话记录。可在一个界面中查看 Claude、Cursor、Codex 等工具的使用情况：token 用量、成本、工具调用、编辑文件以及完整对话历史。

---

## 功能特点

### 多数据源会话发现

- **Claude Code** — 会话位于 `~/.claude/projects`（或你配置的根目录）
- **Cursor IDE** — Cursor 项目会话记录
- **OpenAI Codex CLI** — Codex 会话数据

在 `config.json5` 中配置一个或多个数据源；AINK 会将所有已启用源的会话合并为同一列表，并可按环境启用/禁用或自定义根目录。

### 会话列表与排序

- 表格展示所有会话：项目名、模型、日期、时长、token 数与预估成本
- 支持按日期、token、成本或时长排序，快速找到用量大或最近的会话
- 支持按需从磁盘刷新以加载新会话

### 会话详情

选中任意会话可进入详情视图，包含：

- **统计** — 输入/输出 token 明细、模型名、时长与成本估算
- **对话** — 完整轮次历史（用户消息与助手回复）
- **文件** — 会话中涉及的文件列表（创建、读取或编辑）

通过子标签与快捷键在统计、对话、文件之间切换。

### 成本与用量一览

- 单会话与汇总的 token 用量（输入/输出）
- 基于公开模型定价的成本估算（可配置）
- 工具调用与文件编辑数量，直观了解会话“重量”

### TUI 体验

- 键盘驱动导航与标签切换（概览、会话、分析）
- 通过 `config.json5` 自定义快捷键与样式

---

## 安装与运行

**从发布包：** 在 [Release](https://github.com/YOUR_USERNAME/aink/releases) 下载与系统对应的最新版本，解压后将 `aink` 加入 `PATH`。

**从源码**（需 [Rust](https://rustup.rs) 1.70+）：

```bash
git clone https://github.com/YOUR_USERNAME/aink.git && cd aink && cargo install --path .
```

然后运行：

```bash
aink
```

常用参数：`aink --help`、`aink --version`。

## 配置

配置从系统配置目录加载，并与内置默认值合并：

- **macOS：** `~/Library/Application Support/aink/`
- **Linux：** `~/.config/aink/`
- **Windows：** `%APPDATA%/aink/`

通过环境变量覆盖路径：

```bash
AINK_CONFIG=/path/to/config AINK_DATA=/path/to/data aink
```

多数据源配置示例（JSON5）：

```json5
{
  "sources": [
    { "kind": "Claude", "root_dir": "~/.claude/projects", "enabled": true },
    { "kind": "Cursor", "root_dir": "~/Library/Application Support/Cursor/User", "enabled": true },
    { "kind": "Codex", "root_dir": "~/.codex/sessions", "enabled": true }
  ]
}
```

快捷键与样式可在 `config.json5` 中自定义（首次运行后可在配置目录查看默认配置）。

## 环境变量

| 变量             | 说明               |
|------------------|--------------------|
| `AINK_CONFIG`    | 覆盖配置目录       |
| `AINK_DATA`      | 覆盖数据目录       |
| `AINK_LOG_LEVEL` | 日志级别（默认 `INFO`） |
| `RUST_LOG`       | 另一种日志级别控制 |

日志写入数据目录（如 Linux 下 `~/.local/share/aink/aink.log`）。

## 构建与开发

```bash
cargo build              # 调试构建
cargo build --release    # 发布构建（优化）
cargo test               # 测试
cargo clippy             # 静态检查
cargo run                # 运行
```

## 许可证

本项目采用 [Apache License 2.0](LICENSE) 许可证。
