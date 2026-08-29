import { useCallback, useEffect, useState } from 'react';
import { Button, Modal, Spin } from 'antd';
import { CloseOutlined, FolderOpenOutlined, ReloadOutlined } from '@ant-design/icons';
import type { FileNodeDTO } from '../../../types';
import { filePreview, fileTree } from '../../../services/file';
import { errText, toast } from '../../../utils/feedback';
import { FileTree } from '../../../components/filetree/FileTree';

interface FilePanelProps {
  projectPath: string | null;
  projectName: string | null;
  onClose: () => void;
  /** 「@ 引用到输入框」回调 */
  onReference: (relPath: string) => void;
}

/** 右侧文件面板：面板头 + 文件树 + 文件预览弹窗 */
export function FilePanel({ projectPath, projectName, onClose, onReference }: FilePanelProps) {
  const [nodes, setNodes] = useState<FileNodeDTO[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [preview, setPreview] = useState<{ path: string; content: string; truncated: boolean } | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!projectPath) {
      setNodes([]);
      return;
    }
    setLoading(true);
    setError('');
    try {
      setNodes(await fileTree(projectPath));
    } catch (e) {
      // 项目目录被外部删除/移动等异常：提示重新指定项目
      setError(errText(e));
      toast.error(`文件树加载失败：${errText(e)}`);
    } finally {
      setLoading(false);
    }
  }, [projectPath]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onFile = async (relPath: string) => {
    if (!projectPath) return;
    setPreviewLoading(true);
    try {
      const dto = await filePreview(projectPath, relPath);
      setPreview({ path: relPath, content: dto.content, truncated: dto.truncated });
    } catch (e) {
      toast.error(`文件预览失败：${errText(e)}`);
    } finally {
      setPreviewLoading(false);
    }
  };

  return (
    <aside className="file-panel">
      <div className="fp-header">
        <span className="fp-title">
          <FolderOpenOutlined /> 文件
        </span>
        <span className="fp-root mono" title={projectPath ?? ''}>
          {projectName ?? '未打开'}
        </span>
        <div style={{ flex: 1 }} />
        <button className="icon-btn" title="刷新" disabled={loading} onClick={() => void refresh()}>
          {loading ? <Spin size="small" /> : <ReloadOutlined />}
        </button>
        <button className="icon-btn" title="收起" onClick={onClose}>
          <CloseOutlined />
        </button>
      </div>
      <div className="fp-tree">
        {error ? (
          <div className="session-empty">加载失败：{error}，请重新指定项目</div>
        ) : projectPath ? (
          <FileTree nodes={nodes} onFile={(p) => void onFile(p)} treeKey={projectPath} />
        ) : (
          <div className="session-empty">请先打开项目</div>
        )}
      </div>
      <Modal
        open={preview !== null || previewLoading}
        title={<span className="mono">{preview?.path ?? '加载中…'}</span>}
        width={860}
        onCancel={() => setPreview(null)}
        footer={
          <>
            <Button onClick={() => setPreview(null)}>关闭</Button>
            <Button
              type="primary"
              disabled={!preview}
              onClick={() => {
                if (!preview) return;
                onReference(preview.path);
                setPreview(null);
                toast.success(`已引用 ${preview.path}`);
              }}
            >
              @ 引用到输入框
            </Button>
          </>
        }
      >
        {previewLoading ? (
          <div style={{ textAlign: 'center', padding: 48 }}>
            <Spin />
          </div>
        ) : (
          <>
            <pre className="file-preview mono">{preview?.content ?? ''}</pre>
            {preview?.truncated ? (
              <div style={{ color: 'var(--text-3)', fontSize: 12, marginTop: 8 }}>
                文件超过 64KB，仅展示前部分内容
              </div>
            ) : null}
          </>
        )}
      </Modal>
    </aside>
  );
}
