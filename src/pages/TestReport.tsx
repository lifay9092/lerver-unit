import { useCallback, useEffect, useMemo, useState } from "react";
import {
  App as AntApp,
  Button,
  Card,
  DatePicker,
  Descriptions,
  Drawer,
  Dropdown,
  Empty,
  Form,
  Input,
  Popconfirm,
  Select,
  Space,
  Statistic,
  Table,
  Tag,
  Typography,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  DeleteOutlined,
  DownloadOutlined,
  EyeOutlined,
  ReloadOutlined,
} from "@ant-design/icons";
import dayjs from "dayjs";
import {
  openPath,
  reportDelete,
  reportDetail,
  reportExport,
  reportPage,
  saveFile,
} from "../api/client";
import type { AssertItem, ReportDetail, ReportItemRow, ReportRow } from "../types";

const { RangePicker } = DatePicker;
const { Text, Title, Paragraph } = Typography;

/** 尝试把 JSON 字符串美化；失败则原样返回 */
function pretty(s: string): string {
  if (!s) return "";
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch {
    return s;
  }
}

/** 解析 assert_results 字符串为数组 */
function parseAssert(raw: string): AssertItem[] {
  try {
    return JSON.parse(raw || "[]") as AssertItem[];
  } catch {
    return [];
  }
}

function verdictTag(v: string) {
  if (v === "pass") return <Tag color="success">通过</Tag>;
  if (v === "fail") return <Tag color="error">失败</Tag>;
  return <Tag color="default">不适用</Tag>;
}

export default function TestReport() {
  const { message } = AntApp.useApp();
  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<ReportRow[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);

  const [keyword, setKeyword] = useState("");
  const [hasFail, setHasFail] = useState<"all" | "fail" | "ok">("all");
  const [range, setRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null);

  const [selectedKeys, setSelectedKeys] = useState<number[]>([]);

  const [detail, setDetail] = useState<ReportDetail | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const q: Record<string, unknown> = {
        keyword: keyword || "",
        has_fail: hasFail === "all" ? undefined : hasFail === "fail",
        start: range ? `${range[0].format("YYYY-MM-DD")} 00:00:00` : undefined,
        end: range ? `${range[1].format("YYYY-MM-DD")} 23:59:59` : undefined,
        page,
        page_size: pageSize,
      };
      const res = await reportPage(q as never);
      setRows(res.list);
      setTotal(res.total);
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, [keyword, hasFail, range, page, pageSize, message]);

  useEffect(() => {
    load();
  }, [load]);

  const onDelete = async (ids: number[]) => {
    try {
      const n = await reportDelete(ids);
      message.success(`已删除 ${n} 份报告`);
      setSelectedKeys([]);
      load();
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const onExport = async (r: ReportRow, format: "xlsx" | "json") => {
    const ext = format === "xlsx" ? "xlsx" : "json";
    const path = await saveFile([ext], `${r.name || `report-${r.id}`}.${ext}`);
    if (!path) return;
    try {
      const written = await reportExport(r.id, format, path);
      message.success(`已导出：${written}`);
      // 打开所在目录
      const dir = written.includes("/")
        ? written.substring(0, written.lastIndexOf("/"))
        : written.substring(0, written.lastIndexOf("\\"));
      openPath(dir).catch(() => {});
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const openDetail = async (r: ReportRow) => {
    setDetailOpen(true);
    setDetailLoading(true);
    try {
      const d = await reportDetail(r.id);
      setDetail(d);
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setDetailLoading(false);
    }
  };

  const columns = useMemo<ColumnsType<ReportRow>>(
    () => [
      {
        title: "报告名称",
        dataIndex: "name",
        width: 220,
        render: (n: string) => <Text strong>{n}</Text>,
      },
      {
        title: "结果",
        key: "result",
        width: 160,
        render: (_, r) => (
          <Space size={4}>
            <Tag color="blue">总 {r.total_count}</Tag>
            <Tag color="success">成 {r.success_count}</Tag>
            <Tag color={r.fail_count > 0 ? "error" : "default"}>败 {r.fail_count}</Tag>
          </Space>
        ),
      },
      {
        title: "执行环境",
        key: "env",
        width: 240,
        render: (_, r) => (
          <Space direction="vertical" size={0}>
            <Text>{r.env_name || "未记录"}</Text>
            <Text type="secondary" style={{ fontSize: 12 }}>
              {r.base_url || "-"}
            </Text>
          </Space>
        ),
      },
      { title: "开始时间", dataIndex: "started_at", width: 170 },
      {
        title: "耗时",
        dataIndex: "duration_ms",
        width: 100,
        render: (d: number) => `${d} ms`,
      },
      {
        title: "操作",
        key: "op",
        width: 200,
        render: (_, r) => (
          <Space>
            <Button size="small" icon={<EyeOutlined />} onClick={() => openDetail(r)}>
              查看
            </Button>
            <Dropdown
              menu={{
                items: [
                  { key: "xlsx", label: "导出 xlsx" },
                  { key: "json", label: "导出 json" },
                ],
                onClick: ({ key }) => onExport(r, key as "xlsx" | "json"),
              }}
            >
              <Button size="small" icon={<DownloadOutlined />}>
                导出
              </Button>
            </Dropdown>
            <Popconfirm
              title="确认删除该报告？"
              onConfirm={() => onDelete([r.id])}
              description="明细将一并删除"
            >
              <Button size="small" danger icon={<DeleteOutlined />} />
            </Popconfirm>
          </Space>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [onExport, onDelete],
  );

  const itemColumns: ColumnsType<ReportItemRow> = [
    { title: "用例名称", dataIndex: "case_name", width: 160 },
    { title: "方法", dataIndex: "method", width: 80, render: (m: string) => <Tag>{m}</Tag> },
    { title: "请求地址", dataIndex: "url", width: 280, ellipsis: true },
    {
      title: "状态码",
      dataIndex: "response_status",
      width: 90,
      render: (s: number | null) => (s == null ? <Tag>错误</Tag> : <Text>{s}</Text>),
    },
    {
      title: "结果",
      key: "r",
      width: 100,
      render: (_, it) =>
        it.response_status == null ? (
          <Tag color="warning">错误</Tag>
        ) : it.is_success ? (
          <Tag color="success">通过</Tag>
        ) : (
          <Tag color="error">失败</Tag>
        ),
    },
    {
      title: "耗时",
      dataIndex: "duration_ms",
      width: 90,
      render: (d: number) => `${d} ms`,
    },
    {
      title: "错误信息",
      dataIndex: "error_msg",
      width: 200,
      ellipsis: true,
      render: (e: string) => (e ? <Text type="danger">{e}</Text> : "-"),
    },
  ];

  const rate =
    detail && detail.report.total_count > 0
      ? ((detail.report.success_count / detail.report.total_count) * 100).toFixed(1) + "%"
      : "-";

  return (
    <Card
      title="测试报告"
      extra={
        <Space>
          {selectedKeys.length > 0 && (
            <Popconfirm
              title={`确认删除选中的 ${selectedKeys.length} 份报告？`}
              onConfirm={() => onDelete(selectedKeys)}
            >
              <Button danger icon={<DeleteOutlined />}>
                批量删除
              </Button>
            </Popconfirm>
          )}
          <Button icon={<ReloadOutlined />} onClick={load}>
            刷新
          </Button>
        </Space>
      }
    >
      <Form layout="inline" style={{ marginBottom: 16, gap: 8 }} className="report-filter">
        <Form.Item label="名称">
          <Input
            allowClear
            placeholder="模糊搜索"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            style={{ width: 160 }}
          />
        </Form.Item>
        <Form.Item label="时间范围">
          <RangePicker
            value={range}
            onChange={(v) =>
              setRange(v && v[0] && v[1] ? [v[0], v[1]] : null)
            }
          />
        </Form.Item>
        <Form.Item label="结果">
          <Select
            value={hasFail}
            style={{ width: 120 }}
            onChange={setHasFail}
            options={[
              { value: "all", label: "全部" },
              { value: "fail", label: "含失败" },
              { value: "ok", label: "全成功" },
            ]}
          />
        </Form.Item>
        <Button
          type="primary"
          onClick={() => {
            setPage(1);
            load();
          }}
        >
          查询
        </Button>
      </Form>

      <Table<ReportRow>
        rowKey="id"
        size="middle"
        loading={loading}
        columns={columns}
        dataSource={rows}
        rowSelection={{
          selectedRowKeys: selectedKeys,
          onChange: (keys) => setSelectedKeys(keys as number[]),
        }}
        pagination={{
          current: page,
          pageSize,
          total,
          showSizeChanger: true,
          pageSizeOptions: [10, 20, 50, 100],
          onChange: (p, ps) => {
            setPage(p);
            setPageSize(ps);
          },
        }}
        locale={{ emptyText: <Empty description="暂无报告，去执行用例吧" /> }}
      />

      <Drawer
        title={detail ? `报告详情：${detail.report.name}` : "报告详情"}
        width={880}
        open={detailOpen}
        onClose={() => setDetailOpen(false)}
        loading={detailLoading}
      >
        {detail && (
          <>
            <Space size={24} wrap style={{ marginBottom: 16 }}>
              <Statistic title="执行总数" value={detail.report.total_count} />
              <Statistic title="成功" value={detail.report.success_count} valueStyle={{ color: "#52c41a" }} />
              <Statistic
                title="失败"
                value={detail.report.fail_count}
                valueStyle={{ color: detail.report.fail_count > 0 ? "#ff4d4f" : undefined }}
              />
              <Statistic title="通过率" value={rate} />
              <Statistic title="总耗时" value={`${detail.report.duration_ms} ms`} />
            </Space>
            <Descriptions bordered size="small" column={1} style={{ marginBottom: 16 }}>
              <Descriptions.Item label="执行环境">
                {detail.report.env_name || "未记录"}（{detail.report.base_url || "-"}）
              </Descriptions.Item>
              <Descriptions.Item label="开始时间">{detail.report.started_at}</Descriptions.Item>
              <Descriptions.Item label="创建时间">{detail.report.created_at}</Descriptions.Item>
            </Descriptions>

            <Title level={5}>用例结果（{detail.items.length}）</Title>
            <Table<ReportItemRow>
              rowKey="id"
              size="small"
              columns={itemColumns}
              dataSource={detail.items}
              pagination={false}
              expandable={{
                expandedRowRender: (it) => (
                  <Space direction="vertical" style={{ width: "100%" }} size={8}>
                    <div>
                      <Text strong>请求快照</Text>
                      <Paragraph>
                        <pre style={{ maxHeight: 200, overflow: "auto", margin: 0 }}>{pretty(it.request_snapshot)}</pre>
                      </Paragraph>
                    </div>
                    <div>
                      <Text strong>响应头</Text>
                      <Paragraph>
                        <pre style={{ maxHeight: 160, overflow: "auto", margin: 0 }}>{pretty(it.response_headers)}</pre>
                      </Paragraph>
                    </div>
                    <div>
                      <Text strong>响应体</Text>
                      <Paragraph>
                        <pre style={{ maxHeight: 260, overflow: "auto", margin: 0 }}>{pretty(it.response_body)}</pre>
                      </Paragraph>
                    </div>
                    <div>
                      <Text strong>校验规则</Text>
                      <Space direction="vertical" size={2} style={{ display: "flex" }}>
                        {parseAssert(it.assert_results).map((a, i) => (
                          <span key={i}>
                            {verdictTag(a.verdict)} {a.rule}：{a.message}
                          </span>
                        ))}
                        {parseAssert(it.assert_results).length === 0 && (
                          <Text type="secondary">（无校验规则）</Text>
                        )}
                      </Space>
                    </div>
                  </Space>
                ),
              }}
            />
          </>
        )}
      </Drawer>
    </Card>
  );
}
