import type { ImageDTO } from '../../types';

/** 图片气泡：点击放大（system img），最大高度受限保证消息流可读 */
function ImageBubble({ img }: { img: ImageDTO }) {
  return (
    <img
      className="msg-img"
      src={`data:${img.mime};base64,${img.data}`}
      alt="附件图片"
      loading="lazy"
      onClick={(e) => {
        e.stopPropagation();
        const w = window.open('', '_blank');
        if (w) {
          // 独立查看窗口：直接渲染原图
          w.document.write(
            `<!doctype html><title>图片预览</title><body style="margin:0;background:#14161a;display:flex;align-items:center;justify-content:center;height:100vh"><img style="max-width:96vw;max-height:96vh" src="data:${img.mime};base64,${img.data}"></body>`,
          );
          w.document.close();
        }
      }}
    />
  );
}

/** 用户消息气泡：文本 + 内嵌图片（多模态上传） */
export function UserBubble({ text, images }: { text: string; images?: ImageDTO[] }) {
  const list = images ?? [];
  return (
    <div className="msg-user">
      {list.length > 0 ? (
        <div className={`msg-imgs${list.length > 1 ? ' multi' : ''}`}>
          {list.map((img, i) => (
            <ImageBubble key={`${i}:${img.data.slice(0, 16)}`} img={img} />
          ))}
        </div>
      ) : null}
      {text ? <div className="msg-user-text">{text}</div> : null}
    </div>
  );
}
