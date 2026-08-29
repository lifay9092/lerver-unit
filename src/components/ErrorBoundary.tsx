import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button, Result, Typography } from "antd";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/** 全局错误边界：渲染异常时兜底展示，避免整窗白屏（M7 NFR） */
export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[LeverUnit] 渲染异常:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <Result
          status="error"
          title="页面出现异常"
          subTitle={
            <Typography.Paragraph type="secondary" style={{ maxWidth: 560 }}>
              {this.state.error.message}
              <br />
              请尝试重新加载页面；若持续出现，请反馈该错误信息。
            </Typography.Paragraph>
          }
          extra={
            <Button
              type="primary"
              onClick={() => window.location.reload()}
            >
              重新加载
            </Button>
          }
        />
      );
    }
    return this.props.children;
  }
}
