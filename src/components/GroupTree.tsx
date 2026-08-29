import { Card, Tree, Typography } from "antd";
import type { DataNode } from "antd/es/tree";
import { ApartmentOutlined } from "@ant-design/icons";
import type { GroupNode } from "../types";

interface Props {
  tree: GroupNode[];
  /** 当前选中的分组 id，null 表示「全部用例」 */
  selected: number | null;
  onSelect: (id: number | null) => void;
  loading?: boolean;
}

/** 左侧分组树：含「全部用例」虚拟根节点（FR-C1） */
export default function GroupTree({ tree, selected, onSelect, loading }: Props) {
  const toDataNode = (nodes: GroupNode[]): DataNode[] =>
    nodes.map((n) => ({
      key: n.id,
      title: (
        <span title={n.path}>
          {n.name}
          {n.case_count > 0 && (
            <span style={{ color: "rgba(0,0,0,0.45)", marginLeft: 4 }}>({n.case_count})</span>
          )}
        </span>
      ),
      children: n.children.length ? toDataNode(n.children) : undefined,
    }));

  const treeData: DataNode[] = [
    {
      key: "all",
      title: (
        <span>
          <ApartmentOutlined /> 全部用例
        </span>
      ),
      children: toDataNode(tree),
    },
  ];

  return (
    <Card
      size="small"
      title="分组"
      styles={{ body: { padding: 8, maxHeight: "calc(100vh - 210px)", overflow: "auto" } }}
      loading={loading}
    >
      {tree.length === 0 && !loading && (
        <Typography.Text type="secondary" style={{ fontSize: 12, padding: 4, display: "block" }}>
          暂无分组，请先到「分组管理」创建
        </Typography.Text>
      )}
      <Tree
        treeData={treeData}
        defaultExpandAll
        blockNode
        selectedKeys={[selected === null ? "all" : selected]}
        onSelect={(keys) => {
          const k = keys[0];
          onSelect(k === undefined || k === "all" ? null : Number(k));
        }}
        style={{ background: "transparent" }}
      />
    </Card>
  );
}
