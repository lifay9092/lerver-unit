import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { App as AntApp, Button, Layout, Menu, Select, Space, Tooltip } from "antd";
import {
  ApartmentOutlined,
  DatabaseOutlined,
  ExperimentOutlined,
  FileTextOutlined,
  SettingOutlined,
  ThunderboltOutlined,
} from "@ant-design/icons";
import { HashRouter, Navigate, Route, Routes, useLocation } from "react-router-dom";
import type { EnvFile } from "./types";
import { envList, envReload, envSwitch } from "./api/client";
import Dashboard from "./pages/Dashboard";
import GroupManage from "./pages/GroupManage";
import ImportTemplate from "./pages/ImportTemplate";
import TestCaseManage from "./pages/TestCaseManage";
import TestReport from "./pages/TestReport";
import EnvManager from "./components/EnvManager";
import ErrorBoundary from "./components/ErrorBoundary";

const { Header, Sider, Content } = Layout;

/** 环境上下文：全局共享当前环境配置 */
export const EnvContext = createContext<{
  env: EnvFile | null;
  switchEnv: (id: string) => Promise<void>;
  reloadEnv: () => Promise<void>;
}>({ env: null, switchEnv: async () => {}, reloadEnv: async () => {} });

export const useEnv = () => useContext(EnvContext);

const MENU_ITEMS = [
  { key: "/", icon: <ThunderboltOutlined />, label: "概览" },
  { key: "/groups", icon: <ApartmentOutlined />, label: "分组管理" },
  { key: "/templates", icon: <DatabaseOutlined />, label: "导入模板" },
  { key: "/cases", icon: <ExperimentOutlined />, label: "测试用例" },
  { key: "/reports", icon: <FileTextOutlined />, label: "测试报告" },
];

function EnvSelector() {
  const { env, switchEnv, reloadEnv } = useEnv();
  const [manageOpen, setManageOpen] = useState(false);
  if (!env) return null;
  const activeEnv = env.environments.find((e) => e.id === env.active);
  const headerCount = activeEnv?.common_headers?.length ?? 0;
  return (
    <Space size={12}>
      <span style={{ color: "rgba(0,0,0,0.65)" }}>当前环境：</span>
      <Tooltip title={`当前环境通用请求头：${headerCount} 条（执行/试运行时自动合并到每条用例）`}>
        <Select
          size="small"
          style={{ minWidth: 160 }}
          value={env.active}
          onChange={(id) => switchEnv(id)}
          options={env.environments.map((e) => ({
            value: e.id,
            label: `${e.name} (${e.base_url || "未配置"})`,
          }))}
        />
      </Tooltip>
      <Button
        size="small"
        icon={<SettingOutlined />}
        onClick={() => setManageOpen(true)}
      >
        环境管理
      </Button>
      <a onClick={() => reloadEnv()} style={{ fontSize: 12 }}>
        重新加载
      </a>
      <EnvManager open={manageOpen} onClose={() => setManageOpen(false)} />
    </Space>
  );
}

function Shell() {
  const location = useLocation();
  const selected = MENU_ITEMS.filter(
    (m) => m.key !== "/" && location.pathname.startsWith(m.key),
  );
  return (
    <Layout style={{ minHeight: "100vh" }}>
      <Sider theme="light" width={180} style={{ borderRight: "1px solid #f0f0f0" }}>
        <div
          style={{
            height: 48,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontWeight: 700,
            fontSize: 16,
          }}
        >
          LeverUnit
        </div>
        <Menu
          mode="inline"
          selectedKeys={[selected.length ? selected[selected.length - 1].key : "/"]}
          items={MENU_ITEMS}
          onClick={({ key }) => {
            window.location.hash = `#${key}`;
          }}
        />
      </Sider>
      <Layout>
        <Header
          style={{
            background: "#fff",
            borderBottom: "1px solid #f0f0f0",
            display: "flex",
            justifyContent: "flex-end",
            alignItems: "center",
            paddingInline: 24,
          }}
        >
          <EnvSelector />
        </Header>
        <Content style={{ padding: 16 }}>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/groups" element={<GroupManage />} />
            <Route path="/templates" element={<ImportTemplate />} />
            <Route path="/cases" element={<TestCaseManage />} />
            <Route path="/reports" element={<TestReport />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Content>
      </Layout>
    </Layout>
  );
}

export default function App() {
  const [env, setEnv] = useState<EnvFile | null>(null);

  useEffect(() => {
    envList().then(setEnv).catch(() => setEnv(null));
  }, []);

  const switchEnv = useCallback(async (id: string) => {
    const next = await envSwitch(id);
    setEnv(next);
  }, []);

  const reloadEnv = useCallback(async () => {
    const next = await envReload();
    setEnv(next);
  }, []);

  return (
    <ErrorBoundary>
      <EnvContext.Provider value={{ env, switchEnv, reloadEnv }}>
        <AntApp>
          <HashRouter>
            <Shell />
          </HashRouter>
        </AntApp>
      </EnvContext.Provider>
    </ErrorBoundary>
  );
}
