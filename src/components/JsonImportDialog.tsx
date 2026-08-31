import { useEffect, useMemo, useState } from "react";
import {
  Alert,
  App as AntApp,
  Button,
  Modal,
  Space,
  Table,
  Tag,
  TreeSelect,
  Typography,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  caseImportJsonCommit,
  caseImportJsonPreview,
  groupTree,
  pickFile,
} from "../api/client";
import type {
  GroupNode,
  ImportFailure,
  ImportPreviewRow,
  ImportResult,
} from "../types";

const METHOD_COLOR: Record<string, string> = {
  GET: "blue",
  POST: "green",
  PUT: "orange",
  DELETE: "red",
  PATCH: "purple",
};

interface Props {
  open: boolean;
  /** 预置目标分组（如从分组树「导入到此分组」进入） */
  defaultGroupId?: number | null;
  onClose: () => void;
  onImported: () => void;
}

/** 固定 JSON 格式用例导入弹窗（FR-C13） */
export default function JsonImportDialog({
  open,
  defaultGroupId = null,
  onClose,
  onImported,
}: Props) {
  const { message } = AntApp.useApp();
  const [groupId, setGroupId] = useState<number | null>(defaultGroupId);
  const [filePath, setFilePath] = useState("");
  const [preview, setPreview] = useState<ImportPreviewRow[]>([]);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [groups, setGroups] = useState<GroupNode[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!open) return;
    setGroupId(defaultGroupId);
    setFilePath("");
    setPreview([]);
    setResult(null);
    groupTree()
      .then(setGroups)
      .catch((e) => message.error((e as Error).message));
  }, [open, defaultGroupId, message]);

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

  const pickAndPreview = async () => {
    const path = await pickFile(["json"]);
    if (!path) return;
    setLoading(true);
    try {
      const rows = await caseImportJsonPreview(path);
      setPreview(rows);
      setFilePath(path);
      setResult(null);
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  const doCommit = async () => {
    if (groupId == null || !filePath) {
      message.warning("请完整选择目标分组与文件");
      return;
    }
    setLoading(true);
    try {
      const res = await caseImportJsonCommit(groupId, filePath);
      setResult(res);
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
        <Tag color={METHOD_COLOR[m] || "default"}>{m || "—"}</Tag>
      ),
    },
    { title: "请求地址", dataIndex: ["case_input", "url"], ellipsis: true },
    {
      title: "校验",
      key: "status",
      width: 120,
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

  return (
    <Modal
      title="导入用例（固定 JSON）"
      open={open}
      onCancel={onClose}
      footer={null}
      width={760}
      destroyOnClose
    >
      <Space direction="vertical" style={{ width: "100%" }} size={12}>
        <Typography.Paragraph type="secondary">
          选择目标分组与先前由本系统导出的固定格式 JSON 文件（format=
          <code>lever-unit-case-export</code>
          ）。请求地址为空的用例将被忽略，名称为空的用例将按「分组名_序号」自动命名。
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
          <Alert
            type="warning"
            message="尚未创建任何分组，请先到「分组管理」建分组"
          />
        )}

        <Space>
          <Button onClick={pickAndPreview} loading={loading}>
            选择文件并解析预览
          </Button>
          {filePath && <Typography.Text code>{filePath}</Typography.Text>}
        </Space>

        {preview.length > 0 && !result && (
          <>
            <Alert
              type={
                validCount === preview.length - ignoredCount ? "success" : "warning"
              }
              showIcon
              message={`共解析 ${preview.length} 条，有效 ${validCount} 条，忽略 ${ignoredCount} 条（请求地址为空），失败 ${
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
                r.error ? (
                  <Typography.Text type="danger">{r.error}</Typography.Text>
                ) : null
              }
            />
          </>
        )}

        {result && (
          <>
            <Alert
              type={result.failed.length === 0 ? "success" : "warning"}
              showIcon
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
          </>
        )}

        <Space style={{ justifyContent: "flex-end", width: "100%" }}>
          {!result && (
            <Button
              type="primary"
              loading={loading}
              disabled={groupId == null || !filePath}
              onClick={doCommit}
            >
              开始导入
            </Button>
          )}
          {result && (
            <Button type="primary" onClick={onClose}>
              完成
            </Button>
          )}
        </Space>
      </Space>
    </Modal>
  );
}
