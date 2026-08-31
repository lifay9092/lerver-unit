import { useEffect, useState } from "react";
import {
  App as AntApp,
  Button,
  Empty,
  Form,
  Input,
  Modal,
  Popconfirm,
  Space,
  Table,
  Tag,
  Tooltip,
} from "antd";
import {
  DeleteOutlined,
  EditOutlined,
  FolderOpenOutlined,
  MinusCircleOutlined,
  PlusOutlined,
  ReloadOutlined,
  SaveOutlined,
} from "@ant-design/icons";
import type { EnvFile, EnvItem } from "../types";
import { envReload, envSave, getDataDir, openPath } from "../api/client";
import { useEnv } from "../App";

/**
 * 环境管理小窗（FR-E5）：
 * 查看/新增/编辑/删除环境并写回 environments.json，与手工编辑等价；
 * 支持「重新加载配置」与「打开数据目录」（NFR-3）。
 */
export default function EnvManager({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { message } = AntApp.useApp();
  const { env, reloadEnv } = useEnv();
  const [draft, setDraft] = useState<EnvFile | null>(null);
  const [saving, setSaving] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null); // null=新增
  const [form] = Form.useForm<EnvItem>();

  // 打开时以当前环境配置为草稿
  useEffect(() => {
    if (open && env) {
      setDraft(JSON.parse(JSON.stringify(env)) as EnvFile);
    }
  }, [open, env]);

  const upsert = (item: EnvItem) => {
    setDraft((d) => {
      if (!d) return d;
      const list = [...d.environments];
      if (editingIndex === null) {
        list.push(item);
      } else {
        const oldId = list[editingIndex].id;
        list[editingIndex] = item;
        // id 被修改时同步 active 指向
        if (oldId !== item.id && d.active === oldId) {
          return { ...d, active: item.id, environments: list };
        }
      }
      return { ...d, environments: list };
    });
  };

  const onSave = async () => {
    if (!draft) return;
    setSaving(true);
    try {
      await envSave(draft);
      await reloadEnv(); // 从服务端取回最新配置并刷新全局上下文
      message.success("环境配置已保存");
      onClose();
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const onReload = async () => {
    try {
      const next = await envReload();
      setDraft(JSON.parse(JSON.stringify(next)) as EnvFile);
      await reloadEnv(); // 同步刷新全局上下文
      message.success("已从 environments.json 重新加载");
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const onOpenDataDir = async () => {
    try {
      const dir = await getDataDir();
      await openPath(dir);
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const onDelete = (idx: number) => {
    setDraft((d) => {
      if (!d) return d;
      const list = d.environments.filter((_, i) => i !== idx);
      if (list.length === 0) {
        message.warning("至少需要保留一个环境");
        return d;
      }
      const active = d.active === d.environments[idx].id ? list[0].id : d.active;
      return { ...d, environments: list, active };
    });
  };

  const openEditor = (idx: number | null) => {
    setEditingIndex(idx);
    if (idx === null) {
      form.setFieldsValue({ id: "", name: "", base_url: "", common_headers: [] });
    } else if (draft) {
      const e = draft.environments[idx];
      form.setFieldsValue({
        id: e.id,
        name: e.name,
        base_url: e.base_url,
        common_headers: e.common_headers ?? [],
      });
    }
    setEditorOpen(true);
  };

  const submitEditor = async () => {
    try {
      const values = await form.validateFields();
      // id 唯一性校验（编辑时排除自身）
      const dup = draft?.environments.some(
        (e, i) => e.id === values.id.trim() && i !== editingIndex,
      );
      if (dup) {
        form.setFields([{ name: "id", errors: ["环境 id 已存在"] }]);
        return;
      }
      // 通用请求头：过滤掉 key 为空的行，并校验 key 不重复
      const headers =
        (values.common_headers as { key?: string; value?: string }[] | undefined)?.filter(
          (h) => h && h.key && h.key.trim(),
        ) ?? [];
      const keySet = new Set<string>();
      for (const h of headers) {
        const k = h.key!.trim();
        if (keySet.has(k)) {
          message.error(`通用请求头 key 重复：${k}`);
          return;
        }
        keySet.add(k);
      }
      upsert({
        id: values.id.trim(),
        name: values.name.trim(),
        base_url: values.base_url.trim(),
        common_headers: headers.map((h) => ({ key: h.key!.trim(), value: (h.value ?? "").trim() })),
      });
      setEditorOpen(false);
    } catch {
      /* 表单校验失败 */
    }
  };

  return (
    <Modal
      title="环境管理"
      open={open}
      onCancel={onClose}
      width={680}
      footer={
        <Space>
          <Tooltip title="丢弃修改，重新从 environments.json 读取">
            <Button icon={<ReloadOutlined />} onClick={onReload}>
              重新加载配置
            </Button>
          </Tooltip>
          <Tooltip title="打开数据目录，可直接编辑 environments.json（NFR-3）">
            <Button icon={<FolderOpenOutlined />} onClick={onOpenDataDir}>
              打开数据目录
            </Button>
          </Tooltip>
          <Button onClick={onClose}>取消</Button>
          <Button
            type="primary"
            icon={<SaveOutlined />}
            loading={saving}
            onClick={onSave}
          >
            保存
          </Button>
        </Space>
      }
    >
      <div style={{ marginBottom: 12 }}>
        <Button
          size="small"
          type="dashed"
          icon={<PlusOutlined />}
          onClick={() => openEditor(null)}
        >
          新增环境
        </Button>
      </div>
      <Table<EnvItem>
        rowKey="id"
        size="small"
        pagination={false}
        dataSource={draft?.environments ?? []}
        locale={{ emptyText: <Empty description="暂无环境" /> }}
        columns={[
          {
            title: "激活",
            key: "active",
            width: 64,
            align: "center",
            render: (_, r) =>
              draft?.active === r.id ? <Tag color="green">当前</Tag> : null,
          },
          { title: "id", dataIndex: "id", width: 120 },
          { title: "名称", dataIndex: "name", width: 140 },
          {
            title: "baseUrl",
            dataIndex: "base_url",
            ellipsis: true,
            render: (v: string) => v || <span style={{ color: "#faad14" }}>未配置</span>,
          },
          {
            title: "通用头",
            key: "headers",
            width: 90,
            align: "center",
            render: (_, r) => {
              const n = r.common_headers?.length ?? 0;
              return n > 0 ? <Tag color="blue">{n}</Tag> : <span style={{ color: "rgba(0,0,0,0.25)" }}>0</span>;
            },
          },
          {
            title: "操作",
            key: "op",
            width: 170,
            render: (_, r) => {
              const idx = draft?.environments.findIndex((e) => e.id === r.id) ?? -1;
              return (
                <Space>
                  {draft?.active !== r.id && (
                    <Button
                      size="small"
                      type="link"
                      onClick={() =>
                        setDraft((d) => (d ? { ...d, active: r.id } : d))
                      }
                    >
                      设为当前
                    </Button>
                  )}
                  <Button
                    size="small"
                    icon={<EditOutlined />}
                    onClick={() => openEditor(idx)}
                  />
                  <Popconfirm
                    title="确认删除该环境？"
                    onConfirm={() => onDelete(idx)}
                  >
                    <Button size="small" danger icon={<DeleteOutlined />} />
                  </Popconfirm>
                </Space>
              );
            },
          },
        ]}
      />
      <p style={{ marginTop: 12, color: "rgba(0,0,0,0.45)", fontSize: 12 }}>
        保存后写回 <code>environments.json</code>（与手工编辑文件等价）；切换当前环境即时生效，后续执行与解析均使用新 baseUrl。
      </p>

      <Modal
        title={editingIndex === null ? "新增环境" : "编辑环境"}
        open={editorOpen}
        onCancel={() => setEditorOpen(false)}
        onOk={submitEditor}
        destroyOnHidden
      >
        <Form form={form} layout="vertical">
          <Form.Item
            name="id"
            label="环境 id"
            rules={[{ required: true, message: "请输入环境 id" }]}
            extra="唯一标识，编辑时修改会同步更新激活指向"
          >
            <Input placeholder="如 dev / test / prod" />
          </Form.Item>
          <Form.Item
            name="name"
            label="名称"
            rules={[{ required: true, message: "请输入环境名称" }]}
          >
            <Input placeholder="如 开发环境" />
          </Form.Item>
          <Form.Item name="base_url" label="baseUrl">
            <Input placeholder="如 http://127.0.0.1:8080" />
          </Form.Item>
          <Form.Item
            label="通用请求头"
            extra="执行/试运行时自动合并到每条用例（与用例自身请求头同名时，用例级覆盖环境级）"
          >
            <Form.List name="common_headers">
              {(fields, { add, remove }) => (
                <>
                  {fields.map((field) => (
                    <Space key={field.key} align="baseline" style={{ display: "flex", marginBottom: 8 }}>
                      <Form.Item
                        {...field}
                        name={[field.name, "key"]}
                        rules={[{ required: true, message: "请输入 header 名" }]}
                        style={{ marginBottom: 0 }}
                      >
                        <Input placeholder="Header 名，如 Authorization" />
                      </Form.Item>
                      <Form.Item {...field} name={[field.name, "value"]} style={{ marginBottom: 0 }}>
                        <Input placeholder="Header 值" />
                      </Form.Item>
                      <MinusCircleOutlined onClick={() => remove(field.name)} />
                    </Space>
                  ))}
                  <Button
                    type="dashed"
                    onClick={() => add({ key: "", value: "" })}
                    icon={<PlusOutlined />}
                    size="small"
                  >
                    添加通用请求头
                  </Button>
                </>
              )}
            </Form.List>
          </Form.Item>
        </Form>
      </Modal>
    </Modal>
  );
}
