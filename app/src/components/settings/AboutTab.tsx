/** 设置 - 关于页 */
import logoUrl from '../../assets/logo.png';

export function AboutTab() {
  return (
    <div style={{ textAlign: 'center', padding: '24px 0' }}>
      <img className="about-logo logo-img" src={logoUrl} alt="cyan" />
      <h3 style={{ marginBottom: 6 }}>cyan v1.0.0</h3>
      <p style={{ color: 'var(--text-3)', marginBottom: 20 }}>
        桌面端 AI 编程 Agent · 权限与审批为核心
      </p>
      <div
        style={{
          display: 'flex',
          justifyContent: 'center',
          gap: 24,
          flexWrap: 'wrap',
          textAlign: 'left',
          maxWidth: 560,
          margin: '0 auto',
        }}
      >
        <div>
          <div className="section-title">核心能力</div>
          <p style={{ color: 'var(--text-2)', lineHeight: 2 }}>
            Agent 自主任务循环
            <br />
            文件 / Shell / 搜索工具集
            <br />
            分级权限与审批
            <br />
            上下文自动压缩
          </p>
        </div>
        <div>
          <div className="section-title">本版本覆盖</div>
          <p style={{ color: 'var(--text-2)', lineHeight: 2 }}>
            会话管理（搜索 / 删除 / 新建）
            <br />
            工具调用渲染与 diff 展示
            <br />
            审批流与三种权限模式
            <br />
            模型 / MCP / 权限规则配置
          </p>
        </div>
      </div>
    </div>
  );
}
