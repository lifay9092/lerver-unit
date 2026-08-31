import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Alert,
  App as AntApp,
  Button,
  Modal,
  Select,
  Space,
  Steps,
  Table,
  Tag,
  TreeSelect,
  Typography,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  groupTree,
  importCommit,
  importPreview,
  pickFile,
  templateList,
} from "../api/client";
import {
  type GroupNode,
  type ImportFailure,
  type ImportPreviewRow,
  type ImportResult,
  type TemplateRow,
} from "../types";

const methodColor: Record<string, string> = {
  GET: "blue",
  POST: "green",
  PUT: "orange",
  DELETE: "red",
  PATCH: "purple",
};

interface Props {
  open: boolean;
  onClose: () => void;
  onImported: () => void;
}

export default function ImportWizard({ open, onClose, onImported }: Props) {
  const { message } = AntApp.useApp();
  const [current, setCurrent] = useState(0);
  const [groupId, setGroupId] = useState<number | null>(null);
  const [templateId, setTemplateId] = useState<number | null>(null);
  const [filePath, setFilePath] = useState<string>("");
  const [preview, setPreview] = useState<ImportPreviewRow[]>([]);
  const [result, setResult] = useState<ImportResult | null>(null);

  const [groups, setGroups] = useState<GroupNode[]>([]);
  const [templates, setTemplates] = useState<TemplateRow[]>([]);
  const [loading, setLoading] = useState(false);

  const reset = useCallback(() => {
    setCurrent(0);
    setGroupId(null);
    setTemplateId(null);
    setFilePath("");
    setPreview([]);
    setResult(null);
  }, []);

  // 打开时加载分组树与模板列表
  useEffect(() => {
    if (!open) return;
    reset();
    groupTree()
      .then(setGroups)
      .catch((e) => message.error((e as Error).message));
    templateList({ page: 1, page_size: 200 })
      .then((r) => setTemplates(r.list))
      .catch((e) => message.error((e as Error).message));
  }, [open, reset, message]);

  const treeData = useMemo(() => {
    type TNode = { title: string; value: number; children: TNode[] };
    const build = (nodes: GroupNode[]): TNode[] =>
      nodes.map((n) => ({
        title: `${n.name}（${n.case_count}）`,
        value: n.id,
        children: n.children.length ? build(n.children) : [],
      }));
    return build(groups);
  }, [groups]);

  const selectedTemplate = templates.find((t) => t.id === templateId) || null;

  const pickAndPreview = async () => {
    if (!templateId) {
      message.warning("请先选择模板");
      return;
    }
    const ext = selectedTemplate?.type === "json" ? ["json"] : ["xlsx", "xls"];
    const path = await pickFile(ext);
    if (!path) return;
    setLoading(true);
    try {
      const rows = await importPreview(path, templateId);
      setPreview(rows);
      setFilePath(path);
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  const doCommit = async () => {
    if (groupId == null || !templateId || !filePath) {
      message.warning("请完整选择分组、模板与文件");
      return;
    }
    setLoading(true);
    try {
      const res = await importCommit(groupId, filePath, templateId);
      setResult(res);
      setCurrent(3);
      onImported();
      if (res.failed.length === 0 && res.ignored_count === 0) {
        message.success(`导入成功，共 ${res.success_count} 条`);
      } else {
        message.warning(
          `导入完成：${res.success_count} 条成功，${res.failed.length} 条失败，${res.ignored_count} 条忽略（地址为空）`,
        );
      }
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  const previewColumns: ColumnsType<ImportPreviewRow> = [
    { title: "行号", dataIndex: "row", width: 70 },
    { title: "用例名称", dataIndex: ["case_input", "case_name"], ellipsis: true },
    {
      title: "方法",
      dataIndex: ["case_input", "method"],
      width: 90,
      render: (m: string) => (
        <Tag color={methodColor[m] || "default"}>{m || "—"}</Tag>
      ),
    },
    { title: "请求地址", dataIndex: ["case_input", "url"], ellipsis: true },
    {
      title: "校验",
      key: "status",
      width: 110,
      render: (_, r) =>
        r.status === "error" ? (
          <Tag color="error">无效</Tag>
        ) : r.status === "ignored" ? (
          <Tag color="default">忽略</Tag>
        ) : r.will_auto_name ? (
          <Tag color="purple">有效(自动命名)</Tag>
        ) : (
          <Tag color="success">有效</Tag>
        ),
    },
  ];

  const validCount = preview.filter((r) => r.status === "ok").length;
  const ignoredCount = preview.filter((r) => r.status === "ignored").length;
  const failColumns: ColumnsType<ImportFailure> = [
    { title: "行号", dataIndex: "row", width: 70 },
    { title: "用例名称", dataIndex: "case_name", ellipsis: true },
    { title: "失败原因", dataIndex: "reason", ellipsis: true },
  ];

  const steps = [
    { title: "选择分组" },
    { title: "选择模板" },
    { title: "选择文件并预览" },
    { title: "导入结果" },
  ];

  return (
    <Modal
      title="批量导入用例"
      open={open}
      onCancel={onClose}
      footer={null}
      width={760}
      destroyOnClose
    >
      <Steps current={current} items={steps} size="small" style={{ marginBottom: 20 }} />

      {/* Step 0: 分组 */}
      {current === 0 && (
        <div>
          <Typography.Paragraph type="secondary">
            选择导入用例要归属的目标分组。
          </Typography.Paragraph>
          <TreeSelect
            style={{ width: "100%" }}
            treeData={treeData}
            value={groupId ?? undefined}
            onChange={(v) => setGroupId(v as number)}
            placeholder="请选择目标分组"
            allowClear
            treeDefaultExpandAll
          />
          {groups.length === 0 && (
            <Alert style={{ marginTop: 12 }} type="warning" message="尚未创建任何分组，请先到「分组管理」建分组" />
          )}
        </div>
      )}

      {/* Step 1: 模板 */}
      {current === 1 && (
        <div>
          <Typography.Paragraph type="secondary">
            选择与文件格式匹配的导入模板（xlsx 用表格列头映射，json 用字段路径映射）。
          </Typography.Paragraph>
          <Select
            style={{ width: "100%" }}
            value={templateId ?? undefined}
            onChange={(v) => setTemplateId(v)}
            placeholder="请选择导入模板"
            options={templates.map((t) => ({
              value: t.id,
              label: `${t.name}（${t.type.toUpperCase()}）${t.is_builtin ? " [内置]" : ""}`,
            }))}
          />
        </div>
      )}

      {/* Step 2: 文件 + 预览 */}
      {current === 2 && (
        <div>
          <Space style={{ marginBottom: 12 }}>
            <Button onClick={pickAndPreview} loading={loading}>
              选择文件并解析预览
            </Button>
            {filePath && <Typography.Text code>{filePath}</Typography.Text>}
          </Space>
          {preview.length > 0 && (
            <>
              <Alert
                type={validCount === preview.length - ignoredCount ? "success" : "warning"}
                showIcon
                style={{ marginBottom: 12 }}
                message={`共解析 ${preview.length} 条，有效 ${validCount} 条，忽略 ${
                  ignoredCount
                } 条（请求地址为空），失败 ${
                  preview.length - validCount - ignoredCount
                } 条（失败/忽略行导入时跳过）`}
              />
              <Table<ImportPreviewRow>
                rowKey="row"
                size="small"
                columns={previewColumns}
                dataSource={preview}
                pagination={{ pageSize: 8, showSizeChanger: false }}
                scroll={{ y: 280 }}
                rowClassName={(r) => (r.error ? "row-invalid" : "")}
                expandedRowRender={(r) =>
                  r.error ? <Typography.Text type="danger">{r.error}</Typography.Text> : null
                }
              />
            </>
          )}
        </div>
      )}

      {/* Step 3: 结果 */}
      {current === 3 && result && (
        <div>
          <Alert
            type={result.failed.length === 0 ? "success" : "warning"}
            showIcon
            style={{ marginBottom: 12 }}
            message={`导入完成：成功 ${result.success_count} 条，失败 ${result.failed.length} 条，忽略 ${result.ignored_count} 条（请求地址为空）`}
          />
          {result.failed.length > 0 && (
            <Table<ImportFailure>
              rowKey="row"
              size="small"
              columns={failColumns}
              dataSource={result.failed}
              pagination={{ pageSize: 8, showSizeChanger: false }}
            />
          )}
        </div>
      )}

      <Space style={{ marginTop: 20, justifyContent: "flex-end", width: "100%" }}>
        {current > 0 && current < 3 && (
          <Button onClick={() => setCurrent((c) => c - 1)}>上一步</Button>
        )}
        {current === 0 && (
          <Button type="primary" disabled={groupId == null} onClick={() => setCurrent(1)}>
            下一步
          </Button>
        )}
        {current === 1 && (
          <Button type="primary" disabled={templateId == null} onClick={() => setCurrent(2)}>
            下一步
          </Button>
        )}
        {current === 2 && (
          <Button
            type="primary"
            loading={loading}
            disabled={!filePath}
            onClick={doCommit}
          >
            开始导入
          </Button>
        )}
        {current === 3 && (
          <Button type="primary" onClick={onClose}>
            完成
          </Button>
        )}
      </Space>
    </Modal>
  );
}
