import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Alert,
  App as AntApp,
  Button,
  Card,
  Collapse,
  Descriptions,
  Drawer,
  Dropdown,
  Empty,
  Flex,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Tooltip,
  TreeSelect,
  Typography,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  DownloadOutlined,
  ImportOutlined,
  PlayCircleOutlined,
  PlusOutlined,
  ReloadOutlined,
  SearchOutlined,
  SendOutlined,
} from "@ant-design/icons";
import GroupTree from "../components/GroupTree";
import ImportWizard from "../components/ImportWizard";
import JsonImportDialog from "../components/JsonImportDialog";
import RunDialog from "../components/RunDialog";
import { useEnv } from "../App";
import {
  assertExprValidate,
  caseCreate,
  caseDelete,
  caseExportJson,
  caseIdsByGroup,
  casePage,
  caseUpdate,
  groupTree,
  openPath,
  resolveUrl,
  saveFile,
  tryRun,
} from "../api/client";
import type {
  AssertItem,
  CaseExportInput,
  CaseExportResult,
  CaseInput,
  CaseRow,
  GroupNode,
  TryRunResult,
} from "../types";
import { CONTENT_TYPES, HTTP_METHODS } from "../types";

/** 方法 → 颜色 */
const METHOD_COLOR: Record<string, string> = {
  GET: "green",
  POST: "blue",
  PUT: "orange",
  DELETE: "red",
  PATCH: "purple",
};

/** 校验 JSON 对象格式（前端预校验，Rust 侧会再校验一次） */
const jsonValidator = (label: string) => ({
  validator: (_: unknown, value: string) => {
    if (!value || !value.trim()) return Promise.resolve();
    try {
      const v = JSON.parse(value);
      if (typeof v !== "object" || Array.isArray(v) || v === null) {
        return Promise.reject(new Error(`${label}必须是 JSON 对象，如 {"k":"v"}`));
      }
      return Promise.resolve();
    } catch (e) {
      return Promise.reject(new Error(`${label}不是合法 JSON：${(e as Error).message}`));
    }
  },
});

/** 测试用例管理（M3）：FR-C1~C3、C9、C11 */
export default function TestCaseManage() {
  const { message } = AntApp.useApp();
  const { env } = useEnv();

  const [tree, setTree] = useState<GroupNode[]>([]);
  const [treeLoading, setTreeLoading] = useState(false);
  const [groupId, setGroupId] = useState<number | null>(null);
  const [recursive, setRecursive] = useState(true);
  const [keyword, setKeyword] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [rows, setRows] = useState<CaseRow[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);

  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editing, setEditing] = useState<CaseRow | null>(null);
  const [saving, setSaving] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  // M9：固定 JSON 导入
  const [jsonImportOpen, setJsonImportOpen] = useState(false);
  const [jsonImportGroup, setJsonImportGroup] = useState<number | null>(null);
  const [exporting, setExporting] = useState(false);
  const [form] = Form.useForm<CaseInput>();

  // 执行（M5）
  const [selectedRowKeys, setSelectedRowKeys] = useState<number[]>([]);
  const [runOpen, setRunOpen] = useState(false);
  const [runCaseIds, setRunCaseIds] = useState<number[]>([]);
  const [runTitle, setRunTitle] = useState("");
  const [trying, setTrying] = useState(false);
  const [tryResult, setTryResult] = useState<TryRunResult | null>(null);

  // URL 实时解析预览（FR-C11）
  const urlValue = Form.useWatch("url", form) as string | undefined;
  const [preview, setPreview] = useState<string>("");

  const loadTree = useCallback(async () => {
    setTreeLoading(true);
    try {
      setTree(await groupTree());
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setTreeLoading(false);
    }
  }, [message]);

  const loadCases = useCallback(async () => {
    setLoading(true);
    try {
      const res = await casePage({
        group_id: groupId,
        recursive,
        keyword: keyword || undefined,
        page,
        page_size: pageSize,
      });
      setRows(res.list);
      setTotal(res.total);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setLoading(false);
    }
  }, [groupId, recursive, page, pageSize, keyword, message]);

  useEffect(() => {
    loadTree();
  }, [loadTree]);

  useEffect(() => {
    loadCases();
  }, [loadCases]);

  // URL 预览：URL 变化或切换环境时重新解析
  useEffect(() => {
    const u = (urlValue ?? "").trim();
    if (!u) {
      setPreview("");
      return;
    }
    const timer = setTimeout(() => {
      resolveUrl(u)
        .then(setPreview)
        .catch((e) => setPreview(`解析失败：${e instanceof Error ? e.message : e}`));
    }, 300);
    return () => clearTimeout(timer);
  }, [urlValue, env?.active]);

  const groupOptions = useMemo(() => {
    type Opt = { title: string; value: number; children: Opt[] };
    const build = (nodes: GroupNode[]): Opt[] =>
      nodes.map((n) => ({
        title: n.name,
        value: n.id,
        children: n.children.length ? build(n.children) : [],
      }));
    return build(tree);
  }, [tree]);

  const openCreate = () => {
    setEditing(null);
    form.resetFields();
    form.setFieldsValue({
      group_id: groupId,
      case_name: "",
      method: "GET",
      content_type: "application/json",
      url: "",
      headers: "{}",
      query_params: "{}",
      request_body: "",
      assert_expr: "status == 200",
      timeout_secs: undefined,
      remark: "",
    });
    setDrawerOpen(true);
  };

  const openEdit = (row: CaseRow) => {
    setEditing(row);
    form.setFieldsValue({
      id: row.id,
      group_id: row.group_id,
      case_name: row.case_name,
      method: row.method,
      content_type: row.content_type,
      url: row.url,
      headers: row.headers || "{}",
      query_params: row.query_params || "{}",
      request_body: row.request_body || "",
      assert_expr: row.assert_expr || "",
      timeout_secs: row.timeout_secs,
      remark: row.remark || "",
    });
    setDrawerOpen(true);
  };

  const handleSave = async () => {
    let values: CaseInput;
    try {
      values = await form.validateFields();
    } catch {
      return; // 表单内已展示错误
    }
    setSaving(true);
    try {
      const input: CaseInput = {
        ...values,
        id: editing?.id,
        group_id: values.group_id ?? null,
        headers: values.headers?.trim() || "{}",
        query_params: values.query_params?.trim() || "{}",
        assert_expr: values.assert_expr?.trim() || "",
        timeout_secs: values.timeout_secs,
        remark: values.remark ?? "",
      };
      if (editing) {
        await caseUpdate(input);
        message.success("用例已更新");
      } else {
        await caseCreate(input);
        message.success("用例已创建");
      }
      setDrawerOpen(false);
      await Promise.all([loadCases(), loadTree()]);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (row: CaseRow) => {
    try {
      await caseDelete(row.id);
      message.success(`用例「${row.case_name}」已删除`);
      await Promise.all([loadCases(), loadTree()]);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    }
  };

  const handleCheckExpr = async () => {
    const expr = (form.getFieldValue("assert_expr") as string) ?? "";
    try {
      const n = await assertExprValidate(expr);
      message.success(n === 0 ? "表达式为空（不校验响应）" : `语法正确，共 ${n} 条规则`);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    }
  };

  // —— 执行（M5）——
  const openRunByGroup = async () => {
    try {
      const ids = await caseIdsByGroup(groupId, recursive);
      if (ids.length === 0) {
        message.info("该范围下暂无用例");
        return;
      }
      setRunCaseIds(ids);
      setRunTitle(groupId === null ? `执行全部用例（${ids.length} 条）` : `执行本分组（${ids.length} 条）`);
      setRunOpen(true);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    }
  };

  const openRunSelected = () => {
    if (selectedRowKeys.length === 0) {
      message.info("请先勾选要执行的用例");
      return;
    }
    setRunCaseIds(selectedRowKeys);
    setRunTitle(`执行选中（${selectedRowKeys.length} 条）`);
    setRunOpen(true);
  };

  // —— 导出（M9：固定 JSON）——
  const doExport = async (
    scope: "all" | "group" | "selected",
    explicitGroupId?: number | null,
  ) => {
    try {
      let targetGroupId: number | undefined;
      let caseIds: number[] | undefined;
      if (scope === "selected") {
        if (selectedRowKeys.length === 0) {
          message.info("请先勾选要导出的用例");
          return;
        }
        caseIds = selectedRowKeys;
      } else if (scope === "group") {
        if (explicitGroupId != null) {
          targetGroupId = explicitGroupId;
        } else if (groupId != null) {
          targetGroupId = groupId;
        } else {
          message.info("请先在左侧选择要导出的分组");
          return;
        }
      }
      const defaultName = `lever-unit-cases-${scope}-${Date.now()}.json`;
      const path = await saveFile(["json"], defaultName);
      if (!path) return;
      setExporting(true);
      const input: CaseExportInput = {
        scope: { kind: scope, group_id: targetGroupId, recursive: true, case_ids: caseIds },
        file_path: path,
      };
      const res: CaseExportResult = await caseExportJson(input);
      message.success(`已导出 ${res.count} 条用例到 ${res.file_path}`);
      // 导出后用默认方式打开文件，便于查看
      try {
        await openPath(res.file_path);
      } catch {
        /* 打开失败不阻断导出结果 */
      }
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setExporting(false);
    }
  };

  // —— 固定 JSON 导入（M9）——
  const openJsonImport = (defaultGroup?: number | null) => {
    setJsonImportGroup(defaultGroup ?? null);
    setJsonImportOpen(true);
  };

  const handleTryRun = async () => {
    let values: CaseInput;
    try {
      values = await form.validateFields();
    } catch {
      return;
    }
    setTrying(true);
    try {
      const res = await tryRun({
        id: editing?.id,
        group_id: values.group_id ?? null,
        case_name: values.case_name,
        method: values.method,
        content_type: values.content_type,
        url: values.url,
        headers: values.headers?.trim() || "{}",
        query_params: values.query_params?.trim() || "{}",
        request_body: values.request_body || "",
        assert_expr: values.assert_expr?.trim() || "",
        timeout_secs: values.timeout_secs,
        remark: values.remark ?? "",
      });
      setTryResult(res);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setTrying(false);
    }
  };

  const columns: ColumnsType<CaseRow> = [
    {
      title: "用例名称",
      dataIndex: "case_name",
      width: 200,
      ellipsis: true,
      render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span>,
    },
    {
      title: "方法",
      dataIndex: "method",
      width: 90,
      align: "center",
      render: (v: string) => <Tag color={METHOD_COLOR[v]}>{v}</Tag>,
    },
    {
      title: "请求地址",
      dataIndex: "url",
      ellipsis: true,
      render: (v: string) => <code style={{ fontSize: 12 }}>{v}</code>,
    },
    {
      title: "所属分组",
      dataIndex: "group_path",
      width: 200,
      ellipsis: true,
      render: (v: string | null) =>
        v ? v : <Typography.Text type="secondary">未分组</Typography.Text>,
    },
    {
      title: "超时",
      dataIndex: "timeout_secs",
      width: 80,
      align: "center",
      render: (v: number) => `${v}s`,
    },
    { title: "更新时间", dataIndex: "updated_at", width: 170 },
    {
      title: "操作",
      key: "action",
      width: 120,
      render: (_, row) => (
        <Space size={0}>
          <Button type="link" size="small" onClick={() => openEdit(row)}>
            编辑
          </Button>
          <Popconfirm
            title="确认删除该用例？"
            okText="删除"
            okButtonProps={{ danger: true }}
            onConfirm={() => handleDelete(row)}
          >
            <Button type="link" size="small" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <Flex gap={12} align="flex-start">
      <div style={{ width: 260, flex: "0 0 260px" }}>
        <GroupTree
          tree={tree}
          selected={groupId}
          loading={treeLoading}
          onSelect={(id) => {
            setGroupId(id);
            setPage(1);
          }}
          onExportGroup={(id) => doExport("group", id)}
          onImportGroup={(id) => openJsonImport(id)}
        />
      </div>

      <Card
        style={{ flex: 1 }}
        title="测试用例"
        extra={
          <Space>
            <Tooltip title="勾选后查询包含所有子分组下的用例">
              <Space size={4}>
                <Typography.Text style={{ fontSize: 13 }}>含子分组</Typography.Text>
                <Switch
                  size="small"
                  checked={recursive}
                  onChange={(v) => {
                    setRecursive(v);
                    setPage(1);
                  }}
                />
              </Space>
            </Tooltip>
            <Input
              allowClear
              prefix={<SearchOutlined />}
              placeholder="名称 / URL 模糊搜索"
              style={{ width: 220 }}
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
              onPressEnter={() => {
                setPage(1);
                loadCases();
              }}
            />
            <Button icon={<ReloadOutlined />} onClick={() => Promise.all([loadCases(), loadTree()])}>
              刷新
            </Button>
            <Button icon={<ImportOutlined />} onClick={() => setImportOpen(true)}>
              批量导入
            </Button>
            <Button icon={<ImportOutlined />} onClick={() => openJsonImport()}>
              导入(JSON)
            </Button>
            <Dropdown
              menu={{
                items: [
                  {
                    key: "selected",
                    label: "导出勾选用例",
                    disabled: selectedRowKeys.length === 0,
                  },
                  {
                    key: "group",
                    label: "按当前分组导出",
                    disabled: groupId == null,
                  },
                  { key: "all", label: "导出全部用例" },
                ],
                onClick: ({ key }) => doExport(key as "all" | "group" | "selected"),
              }}
            >
              <Button icon={<DownloadOutlined />} loading={exporting}>
                导出
              </Button>
            </Dropdown>
            <Button icon={<PlayCircleOutlined />} onClick={openRunByGroup}>
              {groupId === null ? "执行全部" : "执行本分组"}
            </Button>
            <Button
              type="primary"
              icon={<PlayCircleOutlined />}
              onClick={openRunSelected}
              disabled={selectedRowKeys.length === 0}
            >
              执行选中{selectedRowKeys.length ? `(${selectedRowKeys.length})` : ""}
            </Button>
            <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
              新建用例
            </Button>
          </Space>
        }
      >
        <Table<CaseRow>
          rowKey="id"
          size="middle"
          loading={loading}
          columns={columns}
          dataSource={rows}
          rowSelection={{
            selectedRowKeys,
            onChange: (keys) => setSelectedRowKeys(keys as number[]),
          }}
          locale={{ emptyText: <Empty description="该分组下暂无用例，点击「新建用例」添加" /> }}
          pagination={{
            current: page,
            pageSize,
            total,
            showSizeChanger: true,
            showTotal: (t) => `共 ${t} 条`,
            onChange: (p, ps) => {
              setPage(p);
              setPageSize(ps);
            },
          }}
        />
      </Card>

      <Drawer
        title={editing ? `编辑用例：${editing.case_name}` : "新建用例"}
        width={720}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        destroyOnHidden
        extra={
          <Space>
            <Button onClick={() => setDrawerOpen(false)}>取消</Button>
            <Button type="primary" loading={saving} onClick={handleSave}>
              保存
            </Button>
          </Space>
        }
      >
        <Form form={form} layout="vertical">
          <Typography.Title level={5}>基本信息</Typography.Title>
          <Form.Item name="group_id" label="所属分组">
            <TreeSelect
              allowClear
              placeholder="未分组"
              treeData={groupOptions}
              treeDefaultExpandAll
              treeNodeFilterProp="title"
            />
          </Form.Item>
          <Form.Item
            name="case_name"
            label="用例名称"
            rules={[{ required: true, message: "请输入用例名称" }, { max: 100, message: "不超过 100 字符" }]}
          >
            <Input placeholder="如：登录成功场景" maxLength={100} />
          </Form.Item>

          <Typography.Title level={5} style={{ marginTop: 24 }}>
            请求信息
          </Typography.Title>
          <Space size={12} style={{ display: "flex" }}>
            <Form.Item name="method" label="请求方法" style={{ width: 140 }}>
              <Select options={HTTP_METHODS.map((m) => ({ value: m, label: m }))} />
            </Form.Item>
            <Form.Item
              name="timeout_secs"
              label="超时时间（秒）"
              style={{ width: 160 }}
              extra="不填则默认 10 秒"
            >
              <InputNumber<number> min={1} max={600} style={{ width: "100%" }} placeholder="默认 10" />
            </Form.Item>
            <Form.Item name="content_type" label="请求内容类型" style={{ flex: 1 }}>
              <Select
                options={CONTENT_TYPES.map((c) => ({ value: c.value, label: c.label }))}
              />
            </Form.Item>
          </Space>
          <Form.Item
            name="url"
            label="请求地址"
            tooltip="支持完整地址 https://api.xxx.com/login、相对路径 /api/login、占位符 {{baseUrl}}/api/login"
            rules={[{ required: true, message: "请输入请求地址" }]}
          >
            <Input placeholder="/api/login" />
          </Form.Item>
          {preview && (
            <Alert
              type={preview.startsWith("解析失败") ? "error" : "success"}
              showIcon
              style={{ marginBottom: 16 }}
              message={
                <span style={{ fontSize: 12 }}>
                  当前环境（{env?.environments.find((e) => e.id === env.active)?.name ?? "未选择"}）解析结果：
                  <code style={{ marginLeft: 4 }}>{preview}</code>
                </span>
              }
            />
          )}
          <Form.Item name="query_params" label="URL 查询参数" rules={[jsonValidator("查询参数")]}>
            <Input.TextArea rows={2} placeholder='{"page":1,"size":20}' />
          </Form.Item>
          <Form.Item name="headers" label="请求头" rules={[jsonValidator("请求头")]}>
            <Input.TextArea rows={3} placeholder='{"Authorization":"Bearer xxx"}' />
          </Form.Item>
          <Form.Item
            name="request_body"
            label="请求体"
            tooltip="按上方「请求内容类型」解析；none 时留空"
          >
            <Input.TextArea rows={5} placeholder='{"username":"admin","password":"123456"}' />
          </Form.Item>

          <Typography.Title level={5} style={{ marginTop: 24 }}>
            校验表达式
          </Typography.Title>
          <Collapse
            size="small"
            style={{ marginBottom: 12 }}
            items={[
              {
                key: "help",
                label: "语法说明（点击展开）",
                children: (
                  <Descriptions size="small" column={1} bordered>
                    <Descriptions.Item label="规则格式">
                      <code>{"<提取器> <比较符> <期望值>"}</code>，多条用 AND 连接（或换行）
                    </Descriptions.Item>
                    <Descriptions.Item label="提取器">
                      <code>status</code>（状态码）、<code>jsonpath:$.code</code>（JSON 路径）、
                      <code>header:X-Token</code>（响应头）
                    </Descriptions.Item>
                    <Descriptions.Item label="比较符">
                      <code>== != &gt; &gt;= &lt; &lt;= contains</code>
                    </Descriptions.Item>
                    <Descriptions.Item label="示例">
                      <code>status == 200 AND jsonpath:$.code == 0 AND jsonpath:$.msg contains 成功</code>
                    </Descriptions.Item>
                    <Descriptions.Item label="说明">
                      非 JSON 响应体时 jsonpath 规则标记「不适用」；留空则不校验响应，仅记录结果
                    </Descriptions.Item>
                  </Descriptions>
                ),
              },
            ]}
          />
          <Form.Item
            name="assert_expr"
            label="校验规则"
            extra={
              <Button type="link" size="small" style={{ paddingLeft: 0 }} onClick={handleCheckExpr}>
                校验语法
              </Button>
            }
          >
            <Input.TextArea rows={3} placeholder="status == 200 AND jsonpath:$.code == 0" />
          </Form.Item>

          <Form.Item name="remark" label="备注">
            <Input.TextArea rows={2} maxLength={200} />
          </Form.Item>

          <Space>
            <Button icon={<SendOutlined />} loading={trying} onClick={handleTryRun}>
              发送试运行
            </Button>
            <Tooltip title="试运行按当前激活环境解析 URL，发送但不生成报告记录，便于调试">
              <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                不落库，仅用于调试
              </Typography.Text>
            </Tooltip>
          </Space>
        </Form>
      </Drawer>

      <RunDialog
        open={runOpen}
        onClose={() => setRunOpen(false)}
        caseIds={runCaseIds}
        title={runTitle}
      />

      <Modal
        title="试运行结果"
        open={tryResult !== null}
        onCancel={() => setTryResult(null)}
        footer={<Button onClick={() => setTryResult(null)}>关闭</Button>}
        width={760}
        destroyOnHidden
      >
        {tryResult && <TryRunDetail result={tryResult} />}
      </Modal>

      <ImportWizard
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onImported={() => {
          setPage(1);
          loadCases();
        }}
      />

      <JsonImportDialog
        open={jsonImportOpen}
        defaultGroupId={jsonImportGroup}
        onClose={() => setJsonImportOpen(false)}
        onImported={() => {
          setPage(1);
          loadCases();
          loadTree();
        }}
      />
    </Flex>
  );
}

/** 试运行结果展示（M5：FR-C10） */
function TryRunDetail({ result }: { result: TryRunResult }) {
  const assertColumns: ColumnsType<AssertItem> = [
    { title: "规则", dataIndex: "rule", ellipsis: true },
    {
      title: "判定",
      dataIndex: "verdict",
      width: 90,
      align: "center",
      render: (_, r) => (
        <Tag color={r.verdict === "pass" ? "green" : r.verdict === "skipped" ? "default" : "red"}>
          {r.verdict === "pass" ? "通过" : r.verdict === "skipped" ? "不适用" : "失败"}
        </Tag>
      ),
    },
    {
      title: "实际值",
      dataIndex: "extracted",
      ellipsis: true,
      render: (v: string) => <code style={{ fontSize: 12 }}>{v}</code>,
    },
    {
      title: "说明",
      dataIndex: "message",
      ellipsis: true,
      render: (v: string) =>
        v ? (
          <Typography.Text type="danger" style={{ fontSize: 12 }}>
            {v}
          </Typography.Text>
        ) : (
          "-"
        ),
    },
  ];

  let prettyBody = result.body;
  try {
    prettyBody = JSON.stringify(JSON.parse(result.body), null, 2);
  } catch {
    /* 非 JSON，原样展示 */
  }

  return (
    <Space direction="vertical" style={{ width: "100%" }} size={12}>
      {result.error_msg && <Alert type="error" showIcon message={result.error_msg} />}
      <Descriptions size="small" column={2} bordered>
        <Descriptions.Item label="解析后地址">
          <code style={{ fontSize: 12 }}>{result.resolved_url}</code>
        </Descriptions.Item>
        <Descriptions.Item label="状态码">{result.status ?? "-"}</Descriptions.Item>
        <Descriptions.Item label="耗时">
          {result.duration_ms ? `${(result.duration_ms / 1000).toFixed(2)}s` : "-"}
        </Descriptions.Item>
        <Descriptions.Item label="响应头">
          {Object.keys(result.headers).length ? (
            <Typography.Text code style={{ fontSize: 12 }}>
              {JSON.stringify(result.headers)}
            </Typography.Text>
          ) : (
            "-"
          )}
        </Descriptions.Item>
      </Descriptions>
      <Typography.Text strong>响应体</Typography.Text>
      <Input.TextArea
        readOnly
        value={prettyBody}
        autoSize={{ minRows: 4, maxRows: 16 }}
        style={{ fontFamily: "monospace", fontSize: 12 }}
      />
      <Typography.Text strong>校验结果</Typography.Text>
      {result.assert_results.length === 0 ? (
        <Typography.Text type="secondary">无校验规则</Typography.Text>
      ) : (
        <Table<AssertItem>
          rowKey={(r) => r.rule}
          size="small"
          pagination={false}
          columns={assertColumns}
          dataSource={result.assert_results}
        />
      )}
    </Space>
  );
}
