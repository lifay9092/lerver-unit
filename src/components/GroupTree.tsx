import { Card, Tooltip, Tree, Typography } from "antd";
import type { DataNode } from "antd/es/tree";
import { ApartmentOutlined, DownloadOutlined, ImportOutlined } from "@ant-design/icons";
import type { GroupNode } from "../types";

interface Props {
  tree: GroupNode[];
  /** 当前选中的分组 id，null 表示「全部用例」 */
  selected: number | null;
  onSelect: (id: number | null) => void;
  loading?: boolean;
  /** 导出该分组（含子分组） */
  onExportGroup?: (id: number) => void;
  /** 导入用例到该分组 */
  onImportGroup?: (id: number) => void;
}

/** 左侧分组树：含「全部用例」虚拟根节点（FR-C1） */
export default function GroupTree({
  tree,
  selected,
  onSelect,
  loading,
  onExportGroup,
  onImportGroup,
}: Props) {
  const renderTitle = (id: number, name: string, caseCount: number) => (
    <span style={{ display: "flex", alignItems: "center", justifyContent: "space-between", width: "100%" }}>
      <span title={name}>
        {name}
        {caseCount > 0 && (
          <span style={{ color: "rgba(0,0,0,0.45)", marginLeft: 4 }}>({caseCount})</span>
        )}
      </span>
      <span className="lu-group-actions" style={{ display: "flex", gap: 6 }}>
        {onImportGroup && (
          <Tooltip title="导入用例到此分组">
            <ImportOutlined
              style={{ color: "#1677ff" }}
              onClick={(e) => {
                e.stopPropagation();
                onImportGroup(id);
              }}
            />
          </Tooltip>
        )}
        {onExportGroup && (
          <Tooltip title="导出该分组（含子分组）">
            <DownloadOutlined
              style={{ color: "rgba(0,0,0,0.45)" }}
              onClick={(e) => {
                e.stopPropagation();
                onExportGroup(id);
              }}
            />
          </Tooltip>
        )}
      </span>
    </span>
  );

  const toDataNode = (nodes: GroupNode[]): DataNode[] =>
    nodes.map((n) => ({
      key: n.id,
      title: renderTitle(n.id, n.name, n.case_count),
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
      className="lu-group-tree"
      size="small"
      title="分组"
      styles={{ body: { padding: 8, maxHeight: "calc(100vh - 210px)", overflow: "auto" } }}
      loading={loading}
    >
      <style>{`
        .lu-group-tree .lu-group-actions { opacity: 0; transition: opacity .15s; }
        .lu-group-tree .ant-tree-node-content-wrapper:hover .lu-group-actions { opacity: 1; }
      `}</style>
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
