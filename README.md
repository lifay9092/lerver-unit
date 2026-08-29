# LeverUnit（lever-unit）

接口测试用例管理桌面工具（Tauri 2.0 + React 19 + Rust + SQLite）。

## 功能

- **分组管理**：无限层级分组树，删除保护/级联，拖拽排序
- **测试用例**：请求编辑（方法/URL/content_type/query/headers/body）、校验表达式（`status` / `jsonpath:$.x.y` / `header:X` × 7 种比较符，AND 组合）、URL 按当前环境实时解析预览
- **导入模板 + 批量导入**：xlsx（中文列头）/ json（字段路径）两种模板；模板可导出为带示例行的可填写文件；批量导入带预览与失败行反馈
- **执行引擎**：串行执行、按当前环境 baseUrl 解析、实时进度事件、单条超时、报告落库；单条「试运行」不落库
- **测试报告**：汇总 + 明细（请求快照/响应/逐条校验结果）、过滤/批量删除、导出 xlsx / json
- **环境配置**：`environments.json` 本地文件驱动，界面增删改 + 快速切换

## 开发运行

```bash
npm install
npm run tauri dev
```

> ⚠️ 本项目前端使用 Vite 7，需要 **Node.js ≥ 20.19（推荐 22）**。Node 20.11 会报 `crypto.hash is not a function`。

## 打包

```bash
npm run tauri build
```

产出 NSIS 安装包（`src-tauri/target/release/bundle/nsis/`）。首次安装会自动创建数据目录与空库。

## 数据目录

`%APPDATA%\com.leverunit.app\`：

- `lever-unit.db` — SQLite 数据库（WAL 模式）
- `environments.json` — 环境配置，示例：

```json
{
  "active": "dev",
  "environments": [
    { "id": "dev",  "name": "开发环境", "base_url": "http://127.0.0.1:8080" },
    { "id": "test", "name": "测试环境", "base_url": "https://test-api.example.com" }
  ]
}
```

- `active`：当前激活环境 id（顶部栏切换即写回）
- 用例 URL 支持三种写法：
  1. 绝对地址 `https://api.example.com/login` → 原样使用
  2. 相对路径 `/api/login` → 拼接当前环境 baseUrl
  3. 占位符 `{{baseUrl}}/api/login` → 替换为当前环境 baseUrl

应用内「环境管理」小窗可直接增删改环境并写回该文件，也可「打开数据目录」手工编辑后点「重新加载」。

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2.0 |
| 前端 | React 19 + TypeScript + Vite 7 + Ant Design 5 + react-router-dom 7 |
| 后端 | Rust（rusqlite bundled / reqwest rustls / calamine / rust_xlsxwriter） |
| 存储 | SQLite（`%APPDATA%\com.leverunit.app\lever-unit.db`） |

## 目录结构

```
src-tauri/src/
  db.rs / migrations.rs   # SQLite 初始化与建表
  env_config.rs           # environments.json 读写 + resolve_url 纯函数
  groups.rs               # 分组 CRUD / 树
  cases.rs                # 用例 CRUD / 分页 / 表达式校验命令
  assert_expr.rs          # 校验表达式解析 + 求值
  executor.rs             # 执行引擎（串行 + 进度事件 + 试运行）
  import.rs               # 导入模板 / 批量导入 / 模板导出
  reports.rs              # 报告查询 / 详情 / 删除 / 导出
src/
  api/client.ts           # invoke 封装 + 命令函数
  components/             # GroupTree / ImportWizard / RunDialog / EnvManager / ErrorBoundary
  pages/                  # Dashboard / GroupManage / TestCaseManage / ImportTemplate / TestReport
docs/                     # 需求文档 / 编码计划
```

## 单元测试

```bash
cd src-tauri && cargo test
```
