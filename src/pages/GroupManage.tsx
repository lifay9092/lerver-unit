import { useCallback, useEffect, useMemo, useState } from "react";
import {
  App as AntApp,
  Button,
  Card,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Space,
  Table,
  Tag,
  Tooltip,
  TreeSelect,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  ArrowDownOutlined,
  ArrowUpOutlined,
  EditOutlined,
  PlusOutlined,
  ReloadOutlined,
  SearchOutlined,
} from "@ant-design/icons";
import { groupCreate, groupDelete, groupList, groupMove, groupUpdate } from "../api/client";
import type { GroupInput, GroupRow } from "../types";

/** 分组管理（M2）：FR-G1 ~ G6 */
export default function GroupManage() {
  const { message, modal } = AntApp.useApp();
  const [loading, setLoading] = useState(false);
  const [rows, setRows] = useState<GroupRow[]>([]);
  const [keyword, setKeyword] = useState("");
  const [pagination, setPagination] = useState({ current: 1, pageSize: 20 });
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<GroupRow | null>(null);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<GroupInput>();

  const load = useCallback(async (kw?: string) => {
    setLoading(true);
    try {
      const list = await groupList(kw ?? keyword);
      setRows(list);
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setLoading(false);
    }
  }, [keyword, message]);

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const treeData = useMemo(() => {
    type TreeNode = { title: string; value: number; children: TreeNode[] };
    const build = (parentId: number | null): TreeNode[] =>
      rows
        .filter((r) => r.parent_id === parentId)
        .map((r) => ({ title: r.name, value: r.id, children: build(r.id) }));
    return build(null);
  }, [rows]);

  /** 编辑时父分组选项中排除自身及其后代（防环） */
  const parentTreeData = useMemo(() => {
    type TreeNode = { title: string; value: number; children: TreeNode[] };
    if (!editing) return treeData;
    const descendants = new Set<number>([editing.id]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const r of rows) {
        if (r.parent_id !== null && descendants.has(r.parent_id) && !descendants.has(r.id)) {
          descendants.add(r.id);
          changed = true;
        }
      }
    }
    const filterTree = (nodes: TreeNode[]): TreeNode[] =>
      nodes
        .filter((n) => !descendants.has(n.value))
        .map((n) => ({ ...n, children: filterTree(n.children) }));
    return filterTree(treeData);
  }, [editing, rows, treeData]);

  const openCreate = (presetParent?: number | null) => {
    setEditing(null);
    form.resetFields();
    form.setFieldsValue({ name: "", parent_id: presetParent ?? null, sort_order: 0, remark: "" });
    setModalOpen(true);
  };

  const openEdit = (row: GroupRow) => {
    setEditing(row);
    form.setFieldsValue({
      name: row.name,
      parent_id: row.parent_id,
      sort_order: row.sort_order,
      remark: row.remark,
    });
    setModalOpen(true);
  };

  const handleSave = async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const input: GroupInput = {
        id: editing?.id,
        name: values.name,
        parent_id: values.parent_id ?? null,
        sort_order: values.sort_order ?? 0,
        remark: values.remark ?? "",
      };
      if (editing) {
        await groupUpdate(input);
        message.success("分组已更新");
      } else {
        await groupCreate(input);
        message.success("分组已创建");
      }
      setModalOpen(false);
      await load();
    } catch (e) {
      message.error(String(e instanceof Error ? e.message : e));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (row: GroupRow) => {
    try {
      await groupDelete(row.id);
      message.success(`分组「${row.name}」已删除`);
      await load();
    } catch (e) {
      // 非空分组等业务拒绝用警告级提示
      modal.warning({ title: "无法删除", content: String(e instanceof Error ? e.message : e) });
    }
  };

  const handleMove = async (row: GroupRow, up: boolean) => {
    try {
      const list = await groupMove(row.id, up);
      setRows(list);
    } catch (e) {
      message.warning(String(e instanceof Error ? e.message : e));
    }
  };

  const columns: ColumnsType<GroupRow> = [
    {
      title: "分组名称",
      dataIndex: "name",
      width: 220,
      render: (name: string, row) => (
        <Space size={4}>
          {row.parent_id !== null && <Tag style={{ marginRight: 0 }}>子</Tag>}
          <span style={{ fontWeight: 500 }}>{name}</span>
        </Space>
      ),
    },
    { title: "全路径", dataIndex: "path", ellipsis: true },
    {
      title: "子分组",
      dataIndex: "child_count",
      width: 90,
      align: "center",
      render: (v: number) => (v > 0 ? <Tag color="blue">{v}</Tag> : v),
    },
    {
      title: "用例数",
      dataIndex: "case_count",
      width: 90,
      align: "center",
      render: (v: number) => (v > 0 ? <Tag color="geekblue">{v}</Tag> : v),
    },
    { title: "排序", dataIndex: "sort_order", width: 80, align: "center" },
    { title: "备注", dataIndex: "remark", width: 180, ellipsis: true, render: (v: string) => v || "-" },
    { title: "更新时间", dataIndex: "updated_at", width: 170 },
    {
      title: "操作",
      key: "action",
      width: 200,
      render: (_, row) => (
        <Space size={0}>
          <Tooltip title="上移">
            <Button type="text" size="small" icon={<ArrowUpOutlined />} onClick={() => handleMove(row, true)} />
          </Tooltip>
          <Tooltip title="下移">
            <Button type="text" size="small" icon={<ArrowDownOutlined />} onClick={() => handleMove(row, false)} />
          </Tooltip>
          <Tooltip title="编辑">
            <Button type="text" size="small" icon={<EditOutlined />} onClick={() => openEdit(row)} />
          </Tooltip>
          <Popconfirm
            title="确认删除该分组？"
            description="存在子分组或用例时将被拒绝"
            okText="删除"
            okButtonProps={{ danger: true }}
            onConfirm={() => handleDelete(row)}
          >
            <Button type="text" size="small" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <Card
      title="分组管理"
      extra={
        <Space>
          <Input
            allowClear
            prefix={<SearchOutlined />}
            placeholder="按名称/路径搜索"
            style={{ width: 220 }}
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            onPressEnter={() => {
              setPagination((p) => ({ ...p, current: 1 }));
              load();
            }}
          />
          <Button icon={<ReloadOutlined />} onClick={() => load()}>
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={() => openCreate(null)}>
            新建分组
          </Button>
        </Space>
      }
    >
      <Table<GroupRow>
        rowKey="id"
        size="middle"
        loading={loading}
        columns={columns}
        dataSource={rows}
        locale={{ emptyText: <Empty description="暂无分组，点击右上角「新建分组」开始" /> }}
        pagination={{
          ...pagination,
          showSizeChanger: true,
          showTotal: (t) => `共 ${t} 个分组`,
          onChange: (current, pageSize) => setPagination({ current, pageSize }),
        }}
      />

      <Modal
        title={editing ? `编辑分组：${editing.name}` : "新建分组"}
        open={modalOpen}
        onOk={handleSave}
        onCancel={() => setModalOpen(false)}
        confirmLoading={saving}
        okText="保存"
        cancelText="取消"
        destroyOnHidden
      >
        <Form form={form} layout="vertical" style={{ marginTop: 16 }}>
          <Form.Item
            name="name"
            label="分组名称"
            rules={[
              { required: true, message: "请输入分组名称" },
              { max: 50, message: "名称不超过 50 字符" },
            ]}
          >
            <Input placeholder="如：用户模块" maxLength={50} />
          </Form.Item>
          <Form.Item name="parent_id" label="父分组" tooltip="不选则为顶级分组；层级上限 5 级">
            <TreeSelect
              allowClear
              placeholder="无（顶级分组）"
              treeData={parentTreeData}
              treeDefaultExpandAll
              treeNodeFilterProp="title"
            />
          </Form.Item>
          <Form.Item name="sort_order" label="排序号" tooltip="同级内按排序号升序，数值越小越靠前">
            <InputNumber min={0} max={9999} style={{ width: "100%" }} />
          </Form.Item>
          <Form.Item name="remark" label="备注">
            <Input.TextArea rows={2} maxLength={200} placeholder="可选" />
          </Form.Item>
        </Form>
      </Modal>
    </Card>
  );
}
