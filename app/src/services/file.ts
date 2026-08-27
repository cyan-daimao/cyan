import { call } from './invoke';
import type { FileNodeDTO, FilePreviewDTO } from '../types';

/** 文件树 / 文件预览命令（TECH_DESIGN 4.1） */

export const fileTree = (projectPath: string) =>
  call<FileNodeDTO[]>('file_tree', { request: { projectPath } });

export const filePreview = (projectPath: string, relPath: string) =>
  call<FilePreviewDTO>('file_preview', { request: { projectPath, relPath } });
