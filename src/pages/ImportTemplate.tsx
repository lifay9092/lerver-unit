import { useCallback, useEffect, useMemo, useState } from "react";
import {
  App as AntApp,
  AutoComplete,
  Button,
  Dropdown,
  Form,
  Input,
  Modal,
  Radio,
  Space,
  Table,
  Tag,
  Typography,
} from "antd";
import type { MenuProps } from "antd";
import { CopyOutlined, DeleteOutlined, EditOutlined, ExportOutlined, MoreOutlined, PlusOutlined, FileSearchOutlined } from "@ant-design/icons";
import type { ColumnsType } from "antd/es/table";
import {
  parseMapping,
  pickFile,
  readJsonFields,
  readXlsxHeaders,
  saveFile,
  stringifyMapping,
  templateCopy,
  templateCreate,
  templateDelete,
  templateExport,
  templateList,
  templateUpdate,
} from "../api/client";
import {
  STD_FIELDS,
  type MappingMap,
  type TemplateInput,
  type TemplateRow,
} from "../types";

const PAGE_SIZE = 10;

export default function ImportTemplate() {
  const { message, modal } = AntApp.useApp();
  const [rows, setRows] = useState<TemplateRow[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [keyword, setKeyword] = useState("");
  const [loading, setLoading] = useState(false);

  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<TemplateRow | null>(null);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<TemplateInput>();
  const [mappings, setMappings] = useState<MappingMap>({});
  const [type, setType] = useState<"xlsx" | "json">("xlsx");
  const [columns, setColumns] = useState<string[]>([]); // 从示例文件读取的列/字段候选

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await templateList({ keyword, page, page_size: PAGE_SIZE });
      setRows(res.list);
      setTotal(res.total);
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, [keyword, page, message]);

  useEffect(() => {
    load();
  }, [load]);

  const openCreate = () => {
    setEditing(null);
    form.setFieldsValue({ name: "", type: "xlsx", mappings: "{}" });
    setMappings({});
    setType("xlsx");
    setColumns([]);
    setModalOpen(true);
  };

  const openEdit = (t: TemplateRow) => {
    setEditing(t);
    const m = parseMapping(t.mappings);
    form.setFieldsValue({ name: t.name, type: t.type, mappings: t.mappings });
    setMappings(m);
    setType(t.type);
    setColumns([]);
    setModalOpen(true);
  };

  /** 从示例文件读取列头/字段，作为映射下拉候选 */
  const loadColumnsFromSample = async () => {
    const path = await pickFile(type === "xlsx" ? ["xlsx", "xls"] : ["json"]);
    if (!path) return;
    try {
      const list = type === "xlsx" ? await readXlsxHeaders(path) : await readJsonFields(path);
      if (list.length === 0) {
        message.warning("未读取到任何列/字段，请检查文件内容");
      } else {
        setColumns(list);
        message.success(`已读取 ${list.length} 个${type === "xlsx" ? "列头" : "字段"}`);
      }
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const onTypeChange = (v: "xlsx" | "json") => {
    setType(v);
    setColumns([]);
  };

  const onOk = async () => {
    try {
      const values = await form.validateFields();
      const input: TemplateInput = {
        id: editing?.id,
        name: values.name.trim(),
        type: values.type,
        mappings: stringifyMapping(mappings),
      };
      setSaving(true);
      if (editing) {
        await templateUpdate(input);
        message.success("模板已更新");
      } else {
        await templateCreate(input);
        message.success("模板已创建");
      }
      setModalOpen(false);
      load();
    } catch (e) {
      if (e instanceof Error) message.error(e.message);
    } finally {
      setSaving(false);
    }
  };

  const onCopy = async (t: TemplateRow) => {
    try {
      await templateCopy(t.id);
      message.success("已复制为可编辑副本");
      load();
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const onDelete = async (t: TemplateRow) => {
    try {
      await templateDelete(t.id);
      message.success("模板已删除");
      load();
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const onExport = async (t: TemplateRow) => {
    const ext = t.type === "xlsx" ? "xlsx" : "json";
    const path = await saveFile([ext], `${t.name}.${ext}`);
    if (!path) return;
    try {
      await templateExport(t.id, path);
      message.success(`模板文件已导出：${path}`);
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  /** 操作列下拉菜单 */
  const buildOpMenu = (t: TemplateRow): MenuProps => ({
    items: [
      { key: "edit", icon: <EditOutlined />, label: "编辑" },
      { key: "copy", icon: <CopyOutlined />, label: "复制" },
      { key: "export", icon: <ExportOutlined />, label: "导出模板" },
      {
        key: "delete",
        icon: <DeleteOutlined />,
        label: "删除",
        danger: true,
        disabled: t.is_builtin,
      },
    ],
    onClick: ({ key, domEvent }) => {
      domEvent.stopPropagation();
      if (key === "edit") openEdit(t);
      else if (key === "copy") onCopy(t);
      else if (key === "export") onExport(t);
      else if (key === "delete") {
        if (t.is_builtin) {
          message.warning("内置模板不可删除，请先复制");
          return;
        }
        modal.confirm({
          title: "确认删除该模板？",
          content: t.name,
          okText: "删除",
          okButtonProps: { danger: true },
          onOk: () => onDelete(t),
        });
      }
    },
  });

  const setSource = (key: string, value: string) => {
    setMappings((m) => ({ ...m, [key]: value }));
  };

  const columnsTable: ColumnsType<TemplateRow> = [
    { title: "模板名称", dataIndex: "name", ellipsis: true },
    {
      title: "类型",
      dataIndex: "type",
      width: 90,
      render: (t: string) => <Tag color={t === "xlsx" ? "blue" : "green"}>{t.toUpperCase()}</Tag>,
    },
    {
      title: "内置",
      dataIndex: "is_builtin",
      width: 80,
      render: (b: boolean) => (b ? <Tag color="gold">内置</Tag> : <Tag>自定义</Tag>),
    },
    {
      title: "已映射字段",
      key: "mapped",
      width: 110,
      render: (_, r) => {
        const n = Object.keys(parseMapping(r.mappings)).length;
        return <span>{n} / {STD_FIELDS.length}</span>;
      },
    },
    { title: "更新时间", dataIndex: "updated_at", width: 170 },
    {
      title: "操作",
      key: "op",
      width: 80,
      align: "center",
      render: (_, r) => (
        <Dropdown menu={buildOpMenu(r)} trigger={["click"]}>
          <Button size="small" icon={<MoreOutlined />} onClick={(e) => e.stopPropagation()}>
            操作
          </Button>
        </Dropdown>
      ),
    },
  ];

  const colOptions = useMemo(
    () => columns.map((c) => ({ value: c, label: c })),
    [columns],
  );

  return (
    <div>
      <Space style={{ marginBottom: 16, justifyContent: "space-between", width: "100%" }}>
        <Typography.Title level={4} style={{ margin: 0 }}>
          导入模板
        </Typography.Title>
        <Space>
          <Input.Search
            placeholder="搜索模板名称"
            allowClear
            onSearch={(v) => {
              setPage(1);
              setKeyword(v);
            }}
            style={{ width: 240 }}
          />
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            新建模板
          </Button>
        </Space>
      </Space>

      <Table<TemplateRow>
        rowKey="id"
        loading={loading}
        columns={columnsTable}
        dataSource={rows}
        pagination={{
          current: page,
          pageSize: PAGE_SIZE,
          total,
          showSizeChanger: false,
          onChange: setPage,
        }}
      />

      <Modal
        title={editing ? `编辑模板${editing.is_builtin ? "（内置副本可改）" : ""}` : "新建模板"}
        open={modalOpen}
        onOk={onOk}
        onCancel={() => setModalOpen(false)}
        confirmLoading={saving}
        width={680}
        destroyOnClose
      >
        <Form form={form} layout="vertical" initialValues={{ type: "xlsx" }}>
          <Form.Item
            name="name"
            label="模板名称"
            rules={[{ required: true, message: "请输入模板名称" }]}
          >
            <Input placeholder="如：通用模板(xlsx中文标题)" />
          </Form.Item>
          <Form.Item name="type" label="模板类型" rules={[{ required: true }]}>
            <Radio.Group onChange={(e) => onTypeChange(e.target.value)}>
              <Radio.Button value="xlsx">xlsx（表格）</Radio.Button>
              <Radio.Button value="json">json（对象数组）</Radio.Button>
            </Radio.Group>
          </Form.Item>

          <Form.Item label="字段映射" style={{ marginBottom: 8 }}>
            <Button
              size="small"
              icon={<FileSearchOutlined />}
              onClick={loadColumnsFromSample}
              style={{ marginBottom: 8 }}
            >
              从示例文件读取{type === "xlsx" ? "列头" : "字段"}
            </Button>
            <Typography.Text type="secondary" style={{ marginLeft: 8, fontSize: 12 }}>
              左侧为标准字段，右侧填写文件中的列名/路径
            </Typography.Text>
          </Form.Item>

          <div style={{ maxHeight: 360, overflowY: "auto" }}>
            <table style={{ width: "100%", borderCollapse: "collapse" }}>
              <thead>
                <tr>
                  <th style={th}>标准字段</th>
                  <th style={th}>文件中的列名 / 字段路径</th>
                </tr>
              </thead>
              <tbody>
                {STD_FIELDS.map((f) => (
                  <tr key={f.key}>
                    <td style={td}>
                      {f.label}
                      {f.required && <span style={{ color: "#ff4d4f" }}> *</span>}
                    </td>
                    <td style={td}>
                      <AutoComplete
                        style={{ width: "100%" }}
                        placeholder={type === "xlsx" ? "如：请求地址" : "如：data.url"}
                        value={mappings[f.key] || ""}
                        options={colOptions}
                        onChange={(v) => setSource(f.key, v)}
                        filterOption={(input, option) =>
                          (option?.value ?? "")
                            .toString()
                            .toLowerCase()
                            .includes(input.toLowerCase())
                        }
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Form>
      </Modal>
    </div>
  );
}

const th: React.CSSProperties = {
  textAlign: "left",
  padding: "6px 8px",
  borderBottom: "1px solid #f0f0f0",
  fontSize: 13,
  background: "#fafafa",
};
const td: React.CSSProperties = {
  padding: "4px 8px",
  borderBottom: "1px solid #f5f5f5",
  fontSize: 13,
};
