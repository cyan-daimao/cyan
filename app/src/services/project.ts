import { call } from './invoke';
import type { ProjectDTO, ProjectTemplate } from '../types';

/** 项目相关命令（TECH_DESIGN 4.1） */

export const listProjects = () => call<ProjectDTO[]>('list_projects');

export const openProject = (path: string) =>
  call<ProjectDTO>('open_project', { request: { path } });

export const createProject = (
  name: string,
  parent: string,
  template: ProjectTemplate,
  gitInit: boolean,
) => call<ProjectDTO>('create_project', { request: { name, parent, template, gitInit } });

export const removeProject = (path: string) =>
  call<void>('remove_project', { request: { path } });
