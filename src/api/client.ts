import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openPath as openPathPlugin } from "@tauri-apps/plugin-opener";
import type {
  ApiResult,
  CaseExportInput,
  CaseExportResult,
  CaseInput,
  CaseQuery,
  CaseRow,
  EnvFile,
  GroupInput,
  GroupNode,
  GroupRow,
  ImportPreviewRow,
  ImportResult,
  MappingMap,
  PageResult,
  ReportDetail,
  ReportQuery,
  ReportRow,
  TemplateInput,
  TemplateQuery,
  TemplateRow,
  TryRunResult,
} from "../types";

/**
 * 统一 invoke 封装：
 * - Rust 侧返回 ApiResult<T>（历史返回结构），code=0 视为成功，否则抛出携带 msg 的 Error
 * - Rust 侧直接 Result::Err 的拒绝同样转为 Error（msg 取其字符串）
 */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    const res = await invoke<ApiResult<T> | T>(cmd, args);
    // 统一返回结构：解包
    if (
      res !== null &&
      typeof res === "object" &&
      "code" in (res as Record<string, unknown>) &&
      "msg" in (res as Record<string, unknown>)
    ) {
      const r = res as ApiResult<T>;
      if (r.code !== 0) {
        throw new Error(r.msg || `命令 ${cmd} 执行失败`);
      }
      return r.data as T;
    }
    return res as T;
  } catch (e) {
    if (e instanceof Error) throw e;
    throw new Error(String(e));
  }
}

// ============ 环境配置 ============

export const envList = () => call<EnvFile>("env_list");

export const envSwitch = (id: string) => call<EnvFile>("env_switch", { id });

export const envReload = () => call<EnvFile>("env_reload");

/** 整体保存环境配置（FR-E5 环境管理小窗） */
export const envSave = (file: EnvFile) => call<EnvFile>("env_save", { file });

/** 数据目录路径（NFR-3） */
export const getDataDir = () => call<string>("data_dir");

export const resolveUrl = (url: string) => call<string>("resolve_url", { url });

// ============ 系统 ============

export const pingDb = () => call<string>("ping_db");

// ============ 分组管理 ============

export const groupList = (keyword?: string) =>
  call<GroupRow[]>("group_list", { keyword: keyword ?? null });

export const groupCreate = (input: GroupInput) =>
  call<GroupRow>("group_create", { input });

export const groupUpdate = (input: GroupInput) =>
  call<GroupRow>("group_update", { input });

export const groupDelete = (id: number) => call<void>("group_delete", { id });

/** 上移/下移：up=true 上移，返回最新列表 */
export const groupMove = (id: number, up: boolean) =>
  call<GroupRow[]>("group_move", { id, up });

/** 分组树（左侧树，含子组数/用例数） */
export const groupTree = () => call<GroupNode[]>("group_tree");

// ============ 测试用例 ============

export const casePage = (query: CaseQuery) =>
  call<PageResult<CaseRow>>("case_page", {
    query: {
      group_id: query.group_id ?? null,
      recursive: query.recursive ?? false,
      keyword: query.keyword ?? "",
      page: query.page ?? 1,
      page_size: query.page_size ?? 20,
    },
  });

export const caseCreate = (input: CaseInput) =>
  call<CaseRow>("case_create", { input });

export const caseUpdate = (input: CaseInput) =>
  call<CaseRow>("case_update", { input });

export const caseDelete = (id: number) => call<void>("case_delete", { id });

/** 校验表达式语法校验，返回规则条数（非法则抛错） */
export const assertExprValidate = (expr: string) =>
  call<number>("assert_expr_validate", { expr });

/** 收集某分组（可选递归）下的全部用例 id（执行前批量取 id） */
export const caseIdsByGroup = (groupId: number | null, recursive: boolean) =>
  call<number[]>("case_ids_by_group", {
    groupId: groupId ?? null,
    recursive,
  });

/** 批量执行用例，返回报告 id（执行过程中通过 run-progress 事件推送进度） */
export const runCases = (caseIds: number[], reportName?: string) =>
  call<number>("run_cases", {
    caseIds,
    reportName: reportName ?? null,
  });

/** 单条试运行（不落库），返回响应与校验结果 */
export const tryRun = (input: CaseInput) => call<TryRunResult>("try_run", { input });

// ============ 导入模板 / 批量导入 ============

export const templateList = (query: TemplateQuery) =>
  call<PageResult<TemplateRow>>("template_list", {
    query: {
      keyword: query.keyword ?? "",
      page: query.page ?? 1,
      page_size: query.page_size ?? 20,
    },
  });

export const templateCreate = (input: TemplateInput) =>
  call<TemplateRow>("template_create", { input });

export const templateUpdate = (input: TemplateInput) =>
  call<TemplateRow>("template_update", { input });

export const templateDelete = (id: number) => call<void>("template_delete", { id });

/** 复制内置/任意模板为可编辑副本 */
export const templateCopy = (id: number) => call<TemplateRow>("template_copy", { id });

/** 导出模板为可填写的模板文件（xlsx/json），filePath 由前端保存对话框确定 */
export const templateExport = (templateId: number, filePath: string) =>
  call<void>("template_export", { templateId, filePath });

/** 读取 xlsx 表头列 */
export const readXlsxHeaders = (filePath: string) =>
  call<string[]>("read_xlsx_headers", { filePath });

/** 读取 json 字段路径 */
export const readJsonFields = (filePath: string) =>
  call<string[]>("read_json_fields", { filePath });

/** 导入预览（不落库） */
export const importPreview = (filePath: string, templateId: number) =>
  call<ImportPreviewRow[]>("import_preview", { filePath, templateId });

/** 批量导入提交：跳过非法行并汇总 */
export const importCommit = (groupId: number, filePath: string, templateId: number) =>
  call<ImportResult>("import_commit", { groupId, filePath, templateId });

// ============ 用例批量导出/导入（固定 JSON，M9） ============

/** 导出用例为固定 JSON 格式到指定路径 */
export const caseExportJson = (input: CaseExportInput) =>
  call<CaseExportResult>("case_export_json", { input });

/** 固定 JSON 导入预览（不落库），返回与模板导入一致的结构 */
export const caseImportJsonPreview = (filePath: string) =>
  call<ImportPreviewRow[]>("case_import_json_preview", { filePath });

/** 固定 JSON 导入提交：跳过非法行并汇总（复用 M8 忽略/自动命名规则） */
export const caseImportJsonCommit = (groupId: number, filePath: string) =>
  call<ImportResult>("case_import_json_commit", { groupId, filePath });

/** 打开文件选择对话框，返回所选路径（取消则为 null） */
export const pickFile = async (extensions: string[]): Promise<string | null> => {
  const selected = await open({
    multiple: false,
    filters: [{ name: "数据文件", extensions }],
  });
  if (Array.isArray(selected)) return selected[0] ?? null;
  return selected;
};

/** 打开文件保存对话框，返回目标路径（取消则为 null） */
export const saveFile = async (
  extensions: string[],
  defaultName?: string,
): Promise<string | null> => {
  const selected = await save({
    filters: [{ name: "模板文件", extensions }],
    defaultPath: defaultName,
  });
  if (Array.isArray(selected)) return selected[0] ?? null;
  return selected;
};

// ============ 测试报告 ============

/** 报告列表（过滤 + 分页） */
export const reportPage = (query: ReportQuery) =>
  call<PageResult<ReportRow>>("report_page", {
    query: {
      keyword: query.keyword ?? "",
      has_fail: query.has_fail ?? null,
      // 注意：空字符串会被后端当成有效过滤条件，必须转成 null
      start: query.start || null,
      end: query.end || null,
      page: query.page ?? 1,
      page_size: query.page_size ?? 20,
    },
  });

/** 报告详情（汇总 + 明细） */
export const reportDetail = (id: number) => call<ReportDetail>("report_detail", { id });

/** 删除报告（单条/批量，级联明细） */
export const reportDelete = (ids: number[]) => call<number>("report_delete", { ids });

/** 导出报告为 xlsx / json 到指定路径，返回写入的文件路径 */
export const reportExport = (id: number, format: "xlsx" | "json", filePath: string) =>
  call<string>("report_export", { id, format, filePath });

/** 用系统默认方式打开文件/目录（导出后用） */
export const openPath = (path: string) => openPathPlugin(path);

// ============ 解析映射便捷函数 ============

/** 把模板 mappings 字符串解析为映射对象 */
export const parseMapping = (raw: string): MappingMap => {
  try {
    return JSON.parse(raw || "{}") as MappingMap;
  } catch {
    return {};
  }
};

/** 把映射对象序列化为模板 mappings 字符串 */
export const stringifyMapping = (m: MappingMap): string => JSON.stringify(m, null, 2);
