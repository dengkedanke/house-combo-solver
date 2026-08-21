import { Component, type ErrorInfo, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  message: string;
}

/** #12：顶层错误边界。任意组件 render 阶段抛错时展示恢复界面，避免整个应用白屏。 */
export default class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, message: '' };

  static getDerivedStateFromError(error: unknown): State {
    return {
      hasError: true,
      message: error instanceof Error ? error.message : String(error),
    };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // 上报/记录：当前仅输出到控制台，便于排查
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div className="error-boundary">
          <h2>程序出现异常</h2>
          <p className="muted">{this.state.message || '未知错误'}</p>
          <button
            className="btn"
            onClick={() => {
              this.setState({ hasError: false, message: '' });
            }}
          >
            重试
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
