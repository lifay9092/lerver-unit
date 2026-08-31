import { useEffect, useRef, useState } from "react";
import {
  App as AntApp,
  Button,
  Input,
  Modal,
  Progress,
  Space,
  Table,
  Tag,
  Typography,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import { PlayCircleOutlined } from "@ant-design/icons";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { runCases } from "../api/client";
import type { RunProgress } from "../types";

interface Props {
  open: boolean;
  onClose: () => void;
  caseIds: number[];
  title: string;
}

interface ProgressRow {
  case_id: number;
  case_name: string;
  phase: string;
  verdict: string;
  detail: string;
  duration_ms: number;
}

const VERDICT_COLOR: Record<string, string> = {
  pass: "green",
  fail: "red",
  error: "volcano",
};

function defaultName(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(
    d.getMinutes(),
  )}${p(d.getSeconds())}`;
}

/** 执行对话框：报告名输入 + 实时进度 + 完成汇总（FR-C6~C8） */
export default function RunDialog({ open, onClose, caseIds, title }: Props) {
  const { message } = AntApp.useApp();
  const [name, setName] = useState("");
  const [running, setRunning] = useState(false);
  const [rows, setRows] = useState<ProgressRow[]>([]);
  const [reportId, setReportId] = useState<number | null>(null);
  const verdictRef = useRef<Record<number, string>>({});

  useEffect(() => {
    if (!open) return;
    let active = true;
    setName("");
    setRows([]);
    setReportId(null);
    verdictRef.current = {};
    const setup = async () => {
      const un: UnlistenFn = await listen<RunProgress>("run-progress", (e) => {
        if (!active) return;
        const p = e.payload;
        if (p.phase === "done" && p.verdict) {
          verdictRef.current[p.case_id] = p.verdict;
        }
        setRows((prev) => {
          const next = [...prev];
          const idx = next.findIndex((r) => r.case_id === p.case_id);
          const row: ProgressRow = {
            case_id: p.case_id,
            case_name: p.case_name,
            phase: p.phase,
            verdict: p.verdict || (idx >= 0 ? next[idx].verdict : ""),
            detail: p.detail,
            duration_ms: p.duration_ms,
          };
          if (idx >= 0) next[idx] = row;
          else next.push(row);
          return next;
        });
      });
      return un;
    };
    let unlisten: UnlistenFn | undefined;
    setup().then((u) => {
      unlisten = u;
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [open]);

  const handleStart = async () => {
    if (caseIds.length === 0) {
      message.warning("没有可执行的用例");
      return;
    }
    setRunning(true);
    setRows([]);
    setReportId(null);
    verdictRef.current = {};
    try {
      const id = await runCases(caseIds, name.trim() || undefined);
      const v = verdictRef.current;
      const success = Object.values(v).filter((x) => x === "pass").length;
      const fail = Object.values(v).filter((x) => x !== "pass").length;
      setReportId(id);
      message.success(`执行完成：成功 ${success} / 失败 ${fail}`);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setRunning(false);
    }
  };

  const columns: ColumnsType<ProgressRow> = [
    { title: "用例名称", dataIndex: "case_name", ellipsis: true },
    {
      title: "状态",
      dataIndex: "verdict",
      width: 100,
      align: "center",
      render: (_, r) =>
        r.phase === "running" ? (
          <Tag color="processing">请求中</Tag>
        ) : r.verdict ? (
          <Tag color={VERDICT_COLOR[r.verdict] ?? "default"}>
            {r.verdict === "pass" ? "通过" : r.verdict === "fail" ? "失败" : "错误"}
          </Tag>
        ) : (
          <Tag>等待</Tag>
        ),
    },
    {
      title: "耗时",
      dataIndex: "duration_ms",
      width: 100,
      align: "center",
      render: (v: number) => (v ? `${(v / 1000).toFixed(2)}s` : "-"),
    },
    {
      title: "说明",
      dataIndex: "detail",
      ellipsis: true,
      render: (v: string) =>
        v ? <Typography.Text type="danger" style={{ fontSize: 12 }}>{v}</Typography.Text> : "-",
    },
  ];

  const done = rows.filter((r) => r.phase === "done").length;
  const percent = caseIds.length ? Math.round((done / caseIds.length) * 100) : 0;

  return (
    <Modal
      title={title}
      open={open}
      onCancel={running ? undefined : onClose}
      footer={
        <Space>
          {reportId !== null && (
            <Typography.Text type="secondary" style={{ marginRight: 8 }}>
              报告已保存（ID: {reportId}），可在「测试报告」查看
            </Typography.Text>
          )}
          <Button onClick={onClose} disabled={running}>
            关闭
          </Button>
          <Button type="primary" icon={<PlayCircleOutlined />} loading={running} onClick={handleStart}>
            {running ? "执行中…" : "开始执行"}
          </Button>
        </Space>
      }
      width={680}
      destroyOnHidden
    >
      <Space direction="vertical" style={{ width: "100%" }} size={12}>
        <Input
          addonBefore="报告名称"
          placeholder="留空则使用时间戳 yyyyMMdd-HHmmss"
          value={name}
          onChange={(e) => setName(e.target.value)}
          defaultValue={defaultName()}
          disabled={running}
        />
        <Typography.Text type="secondary">
          共 {caseIds.length} 条用例，按当前激活环境串行执行（单条超时默认 10s）
        </Typography.Text>
        <Progress percent={percent} status={running ? "active" : "normal"} />
        <Table<ProgressRow>
          rowKey="case_id"
          size="small"
          pagination={false}
          columns={columns}
          dataSource={rows}
          locale={{ emptyText: "尚未开始执行" }}
          scroll={{ y: 280 }}
        />
      </Space>
    </Modal>
  );
}
