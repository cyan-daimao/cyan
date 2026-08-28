import { call } from './invoke';
import type { MarketItemDTO, PluginDTO } from '../types';

/** 插件相关命令（PLUGIN_DESIGN 第 3 节，serde camelCase） */

export const listPlugins = () => call<PluginDTO[]>('list_plugins');

/** 安装：sourcePath 为 zip 文件或插件目录路径 */
export const installPlugin = (sourcePath: string) =>
  call<PluginDTO>('install_plugin', { request: { sourcePath } });

export const togglePlugin = (id: number, enable: boolean) =>
  call<PluginDTO>('toggle_plugin', { request: { id, enable } });

/** 卸载：连带摘除其技能 / MCP / 规则 */
export const deletePlugin = (id: number) => call<void>('delete_plugin', { request: { id } });

/** 插件市场：搜索 GitHub 上的插件仓库（keyword 空串返回推荐列表） */
export const searchMarketplace = (keyword: string) =>
  call<MarketItemDTO[]>('search_marketplace', { request: { keyword } });

/** 从 GitHub 仓库一键安装（fullName = owner/repo） */
export const installPluginFromGithub = (fullName: string) =>
  call<PluginDTO>('install_plugin_from_github', { request: { fullName } });
