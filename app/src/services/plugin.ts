import { call } from './invoke';
import type { MarketItemDTO, MarketSource, PluginDTO } from '../types';

/** 插件相关命令（PLUGIN_DESIGN 第 3 节，serde camelCase） */

export const listPlugins = () => call<PluginDTO[]>('list_plugins');

/** 安装：sourcePath 为 zip 文件或插件目录路径 */
export const installPlugin = (sourcePath: string) =>
  call<PluginDTO>('install_plugin', { request: { sourcePath } });

export const togglePlugin = (id: number, enable: boolean) =>
  call<PluginDTO>('toggle_plugin', { request: { id, enable } });

/** 卸载：连带摘除其技能 / MCP / 规则 */
export const deletePlugin = (id: number) => call<void>('delete_plugin', { request: { id } });

/** 插件市场搜索（GitHub topic:cyan-plugin 搜索 / Gitee owner-repo 直达；keyword 空串时 GitHub 返回推荐列表） */
export const searchMarketplace = (keyword: string, source: MarketSource = 'github') =>
  call<MarketItemDTO[]>('search_marketplace', { request: { keyword, source } });

/** 从远端仓库一键安装（source=gitee 走 Gitee 归档下载，缺省 GitHub） */
export const installPluginFromGithub = (fullName: string, source: MarketSource = 'github') =>
  call<PluginDTO>('install_plugin_from_github', { request: { fullName, source } });
