import { useEffect } from 'react';
import InputPanel from './components/InputPanel';
import GridCanvas from './components/GridCanvas';
import ResultPanel from './components/ResultPanel';
import Sandglass from './components/Sandglass';
import Logo from './components/Logo';
import { useAppStore } from './store/useAppStore';

export default function App() {
  const houseTypes = useAppStore((s) => s.houseTypes);
  const combinations = useAppStore((s) => s.combinations);
  const manualInputs = useAppStore((s) => s.manualInputs);
  const autoCalc = useAppStore((s) => s.autoCalc);
  const setAutoCalc = useAppStore((s) => s.setAutoCalc);
  const solve = useAppStore((s) => s.solve);
  const loadConfig = useAppStore((s) => s.loadConfig);
  const saveConfig = useAppStore((s) => s.saveConfig);
  const loadSample = useAppStore((s) => s.loadSample);
  const error = useAppStore((s) => s.error);
  const calculating = useAppStore((s) => s.calculating);
  const rendering = useAppStore((s) => s.rendering);

  // 启动时加载本地配置
  useEffect(() => {
    loadConfig();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 自动计算（防抖 300ms）
  useEffect(() => {
    if (!autoCalc) return;
    const timer = setTimeout(() => {
      solve();
    }, 300);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [houseTypes, combinations, manualInputs, autoCalc]);

  return (
    <div className="app">
      <header className="topbar">
        <div className="topbar-title">
          <Logo size={24} />
          <h1>房源组合最优解计算器</h1>
        </div>
        <div className="topbar-actions">
          <label className="auto-toggle">
            <input
              type="checkbox"
              checked={autoCalc}
              onChange={(e) => setAutoCalc(e.target.checked)}
            />
            <span>自动计算</span>
          </label>
          <button className="btn btn-ghost" onClick={loadSample}>
            加载示例
          </button>
          <button className="btn btn-ghost" onClick={() => saveConfig()}>
            保存配置
          </button>
        </div>
      </header>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => useAppStore.setState({ error: null })}
          >
            关闭
          </button>
        </div>
      )}

      <div className="layout">
        <InputPanel />
        <GridCanvas />
        <ResultPanel />
      </div>

      {(calculating || rendering) && <Sandglass />}
    </div>
  );
}
