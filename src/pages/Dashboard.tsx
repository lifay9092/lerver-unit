import { useEffect, useState } from "react";
import { Card, Descriptions, Tag, Typography } from "antd";
import { pingDb } from "../api/client";
import { useEnv } from "../App";

/** 概览页：M1 冒烟验证（数据库链路 + 环境配置链路） */
export default function Dashboard() {
  const [dbInfo, setDbInfo] = useState<string>("检测中…");
  const [dbError, setDbError] = useState<string>("");
  const { env } = useEnv();

  useEffect(() => {
    pingDb()
      .then(setDbInfo)
      .catch((e) => {
        setDbInfo("-");
        setDbError(String(e.message ?? e));
      });
  }, []);

  const activeEnv = env?.environments.find((e) => e.id === env.active);

  return (
    <div style={{ maxWidth: 900 }}>
      <Typography.Title level={4}>概览</Typography.Title>
      <Card title="系统状态（M1 冒烟验证）" style={{ marginBottom: 16 }}>
        <Descriptions column={1} size="small">
          <Descriptions.Item label="数据库">
            {dbError ? <Tag color="red">连接失败：{dbError}</Tag> : <Tag color="green">{dbInfo}</Tag>}
          </Descriptions.Item>
          <Descriptions.Item label="环境配置文件">
            {env ? <Tag color="green">已加载（{env.environments.length} 个环境）</Tag> : <Tag color="red">未加载</Tag>}
          </Descriptions.Item>
          <Descriptions.Item label="当前环境">
            {activeEnv ? `${activeEnv.name} → ${activeEnv.base_url || "（未配置 baseUrl）"}` : "-"}
          </Descriptions.Item>
        </Descriptions>
      </Card>
      <Card title="模块开发进度">
        <Descriptions column={1} size="small">
          <Descriptions.Item label="M1 骨架与数据层">
            <Tag color="green">已完成</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="M2 分组管理">
            <Tag>待开发</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="M3 测试用例管理">
            <Tag>待开发</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="M4 导入模板 + 批量导入">
            <Tag>待开发</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="M5 执行引擎 + 校验表达式">
            <Tag>待开发</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="M6 测试报告 + 导出">
            <Tag>待开发</Tag>
          </Descriptions.Item>
        </Descriptions>
      </Card>
    </div>
  );
}
