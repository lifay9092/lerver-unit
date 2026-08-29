/** 与 Rust 侧结构体一一对应（src-tauri/src/*.rs） */

/** 统一命令返回结构（error.rs ApiResult） */
export interface ApiResult<T> {
  code: number;
  msg: string;
  data: T | null;
}

/** 分页返回结构（error.rs PageResult） */
export interface PageResult<T> {
  list: T[];
  total: number;
  page: number;
  page_size: number;
}

/** 分页查询参数 */
export interface PageQuery {
  page: number;
  page_size: number;
  keyword?: string;
}

/** 环境定义（env_config.rs EnvItem） */
export interface EnvItem {
  id: string;
  name: string;
  base_url: string;
}

/** 环境配置文件（env_config.rs EnvFile） */
export interface EnvFile {
  active: string;
  environments: EnvItem[];
}

/** 分组行（groups.rs GroupRow） */
export interface GroupRow {
  id: number;
  name: string;
  parent_id: number | null;
  sort_order: number;
  remark: string;
  created_at: string;
  updated_at: string;
  /** 全路径，如 "项目A / 用户模块 / 登录" */
  path: string;
  child_count: number;
  case_count: number;
}

/** 分组创建/编辑入参（groups.rs GroupInput） */
export interface GroupInput {
  id?: number;
  name: string;
  parent_id: number | null;
  sort_order?: number;
  remark?: string;
}

/** 分组树节点（groups.rs GroupNode） */
export interface GroupNode {
  id: number;
  name: string;
  path: string;
  child_count: number;
  case_count: number;
  children: GroupNode[];
}

/** 用例行（cases.rs CaseRow） */
export interface CaseRow {
  id: number;
  group_id: number | null;
  group_path: string | null;
  case_name: string;
  method: string;
  content_type: string;
  url: string;
  headers: string;
  query_params: string;
  request_body: string;
  assert_expr: string;
  timeout_secs: number;
  remark: string;
  created_at: string;
  updated_at: string;
}

/** 用例创建/编辑入参（cases.rs CaseInput） */
export interface CaseInput {
  id?: number;
  group_id?: number | null;
  case_name: string;
  method: string;
  content_type: string;
  url: string;
  headers?: string;
  query_params?: string;
  request_body?: string;
  assert_expr?: string;
  timeout_secs?: number;
  remark?: string;
}

/** 用例分页查询参数（cases.rs CaseQuery） */
export interface CaseQuery {
  group_id?: number | null;
  recursive?: boolean;
  keyword?: string;
  page?: number;
  page_size?: number;
}

/** 请求方法（与 cases.rs METHODS 一致） */
export const HTTP_METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH"] as const;

/** 请求内容类型（与 cases.rs CONTENT_TYPES 一致） */
export const CONTENT_TYPES = [
  { value: "none", label: "none（无请求体）" },
  { value: "application/json", label: "application/json" },
  {
    value: "application/x-www-form-urlencoded",
    label: "x-www-form-urlencoded",
  },
  { value: "multipart/form-data", label: "form-data" },
] as const;

// ============ 导入模板（import.rs） ============

/** 标准 HTTP 用例字段（与 import.rs STD_FIELDS 一致）：字段名、显示名、是否必填 */
export const STD_FIELDS: { key: string; label: string; required: boolean }[] = [
  { key: "case_name", label: "用例名称", required: true },
  { key: "method", label: "请求方法", required: true },
  { key: "url", label: "请求地址", required: true },
  { key: "content_type", label: "内容类型", required: false },
  { key: "headers", label: "请求头(JSON)", required: false },
  { key: "query_params", label: "查询参数(JSON)", required: false },
  { key: "request_body", label: "请求体", required: false },
  { key: "assert_expr", label: "校验表达式", required: false },
  { key: "timeout_secs", label: "超时(秒)", required: false },
  { key: "remark", label: "备注", required: false },
];

/** 模板行（import.rs TemplateRow），mappings 为 JSON 字符串 */
export interface TemplateRow {
  id: number;
  name: string;
  type: "xlsx" | "json";
  mappings: string;
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
}

/** 模板创建/编辑入参（import.rs TemplateInput） */
export interface TemplateInput {
  id?: number;
  name: string;
  type: "xlsx" | "json";
  mappings: string;
}

/** 模板分页查询参数 */
export interface TemplateQuery {
  keyword?: string;
  page: number;
  page_size: number;
}

/** 导入预览行（import.rs ImportPreviewRow） */
export interface ImportPreviewRow {
  row: number;
  case_input: CaseInput;
  error: string | null;
}

/** 导入失败项（import.rs ImportFailure） */
export interface ImportFailure {
  row: number;
  case_name: string;
  reason: string;
}

/** 导入结果汇总（import.rs ImportResult） */
export interface ImportResult {
  success_count: number;
  failed: ImportFailure[];
}

/** 标准字段 → 文件列/路径 的映射（前端维护用） */
export type MappingMap = Record<string, string>;

// ============ 执行引擎（executor.rs，M5） ============

/** 单条校验结果（assert_expr.rs AssertItem） */
export interface AssertItem {
  rule: string;
  /** pass（通过）/ fail（失败）/ skipped（不适用） */
  verdict: string;
  /** 实际提取值 */
  extracted: string;
  expected: string;
  message: string;
}

/** 实时进度事件（executor.rs RunProgress，事件名 run-progress） */
export interface RunProgress {
  report_id: number;
  total: number;
  current: number;
  case_id: number;
  case_name: string;
  /** running（请求中）/ done（已完成） */
  phase: string;
  /** 完成时判定：pass / fail / error */
  verdict: string;
  detail: string;
  duration_ms: number;
}

/** 试运行结果（executor.rs TryRunResult） */
export interface TryRunResult {
  resolved_url: string;
  status: number | null;
  headers: Record<string, string>;
  body: string;
  duration_ms: number;
  error_msg: string;
  assert_results: AssertItem[];
}

// ============ 测试报告（reports.rs，M6） ============

/** 报告行（reports.rs ReportRow） */
export interface ReportRow {
  id: number;
  name: string;
  total_count: number;
  success_count: number;
  fail_count: number;
  env_name: string;
  base_url: string;
  started_at: string;
  duration_ms: number;
  created_at: string;
}

/** 报告明细行（reports.rs ReportItemRow） */
export interface ReportItemRow {
  id: number;
  report_id: number;
  case_id: number | null;
  case_name: string;
  method: string;
  url: string;
  request_snapshot: string;
  response_status: number | null;
  response_headers: string;
  response_body: string;
  assert_results: string;
  is_success: boolean;
  error_msg: string;
  duration_ms: number;
}

/** 报告详情（reports.rs ReportDetail） */
export interface ReportDetail {
  report: ReportRow;
  items: ReportItemRow[];
}

/** 报告列表查询参数（reports.rs ReportQuery） */
export interface ReportQuery {
  keyword?: string;
  /** true=仅含失败，false=仅全成功，undefined=全部 */
  has_fail?: boolean;
  /** started_at >= start */
  start?: string;
  /** started_at <= end */
  end?: string;
  page: number;
  page_size: number;
}
