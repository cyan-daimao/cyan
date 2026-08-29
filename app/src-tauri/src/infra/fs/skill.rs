//! 技能文件存取：两级目录扫描 + frontmatter 手写解析（不引 yaml 库）。
//! 全局 `~/.cyan/skills/*.md`，项目级 `<项目根>/.cyan/skills/*.md`（PLUGIN_DESIGN 2.2）。

use std::path::{Path, PathBuf};

use crate::domain::shared::ProjectPath;
use crate::domain::skill::{Skill, SkillSource};
use crate::domain::DomainError;

use crate::infra::db::datasource::cyan_home;

/// 全局技能目录（`~/.cyan/skills`）
pub fn global_skills_dir() -> anyhow::Result<PathBuf> {
    Ok(cyan_home()?.join("skills"))
}

/// 项目级技能目录（`<项目根>/.cyan/skills`，经 ProjectPath 校验在项目内）
pub fn project_skills_dir(root: &ProjectPath) -> Result<PathBuf, DomainError> {
    root.resolve(".cyan/skills")
}

/// 按作用域定位技能目录（global 目录不存在时返回路径但不创建）
pub fn skills_dir(source: &SkillSource, root: Option<&ProjectPath>) -> Result<PathBuf, DomainError> {
    match source {
        SkillSource::Global => global_skills_dir()
            .map_err(|e| DomainError::Validation(format!("定位全局技能目录失败：{e}"))),
        SkillSource::Project => {
            let root = root
                .ok_or_else(|| DomainError::Validation("项目级技能需要 projectPath".into()))?;
            project_skills_dir(root)
        }
        SkillSource::Plugin(name) => Err(DomainError::Validation(format!(
            "插件技能目录由插件服务管理：{name}"
        ))),
    }
}

/// 扫描目录下全部 `.md` 技能文件；目录不存在时返回空列表
pub fn scan_skills(dir: &Path, source: SkillSource) -> Result<Vec<Skill>, DomainError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| DomainError::Validation(format!("读取技能目录失败：{e}")))?;
    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // 坏文件跳过，不让单个文件拖垮列表
        };
        match parse_skill(&id, &text, source.clone()) {
            Ok(skill) => skills.push(skill),
            Err(e) => tracing::warn!(id, error = %e, "跳过非法技能文件"),
        }
    }
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(skills)
}

/// 解析技能文件：`---` 包围的 `key: value` frontmatter（name/description/enabled/market）+ 正文
pub fn parse_skill(id: &str, text: &str, source: SkillSource) -> Result<Skill, DomainError> {
    Skill::validate_id(id)?;
    let mut name = id.to_string();
    let mut description = String::new();
    let mut enabled = true;
    let mut market_repo: Option<String> = None;
    let mut content = text.to_string();

    let mut lines = text.lines();
    if lines.next().map(|l| l.trim()) == Some("---") {
        let mut body_start = 0usize;
        let mut offset = 4usize; // 跳过首行 "---\n"
        for line in lines {
            let trimmed = line.trim();
            if trimmed == "---" {
                body_start = offset + line.len() + 1;
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                match key.trim() {
                    "name" => name = value.to_string(),
                    "description" => description = value.to_string(),
                    "enabled" => enabled = value != "false",
                    // 市场来源仓库（owner/repo）
                    "market" => market_repo = (!value.is_empty()).then(|| value.to_string()),
                    _ => {} // 未知键忽略，向前兼容
                }
            }
            offset += line.len() + 1;
        }
        if body_start > 0 {
            content = text.get(body_start..).unwrap_or("").trim().to_string();
        }
    }

    let skill = Skill {
        id: id.to_string(),
        name,
        description,
        enabled,
        source,
        market_repo,
        content,
    };
    skill.validate()?;
    Ok(skill)
}

/// 序列化技能为 Markdown 文件内容（frontmatter + 正文；market 仅 Some 时写入）
pub fn serialize_skill(
    name: &str,
    description: &str,
    enabled: bool,
    market_repo: Option<&str>,
    content: &str,
) -> String {
    let market_line = market_repo
        .map(|m| format!("market: {m}\n"))
        .unwrap_or_default();
    format!("---\nname: {name}\ndescription: {description}\nenabled: {enabled}\n{market_line}---\n{content}\n")
}

/// 保存技能文件：按作用域写 `<dir>/<id>.md`（自动建目录；项目级路径经 ProjectPath 校验）
pub fn save_skill_file(
    source: SkillSource,
    root: Option<&ProjectPath>,
    skill: &Skill,
) -> Result<(), DomainError> {
    let dir = skills_dir(&source, root)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| DomainError::Validation(format!("创建技能目录失败：{e}")))?;
    // 项目级二次防逃逸：目标文件必须仍在项目根内
    let file = match (&source, root) {
        (SkillSource::Project, Some(r)) => r.resolve(&format!(".cyan/skills/{}.md", skill.id))?,
        _ => dir.join(format!("{}.md", skill.id)),
    };
    let text = serialize_skill(&skill.name, &skill.description, skill.enabled, skill.market_repo.as_deref(), &skill.content);
    std::fs::write(&file, text).map_err(|e| DomainError::Validation(format!("写入技能文件失败：{e}")))
}

/// 删除技能文件（幂等：不存在视为成功）
pub fn delete_skill_file(
    source: SkillSource,
    root: Option<&ProjectPath>,
    id: &str,
) -> Result<(), DomainError> {
    Skill::validate_id(id)?;
    let dir = skills_dir(&source, root)?;
    let file = match (&source, root) {
        (SkillSource::Project, Some(r)) => r.resolve(&format!(".cyan/skills/{id}.md"))?,
        _ => dir.join(format!("{id}.md")),
    };
    if file.exists() {
        std::fs::remove_file(&file)
            .map_err(|e| DomainError::Validation(format!("删除技能文件失败：{e}")))?;
    }
    Ok(())
}

/// 由文件路径推导技能 id：`*/SKILL.md`（大小写不敏感）取目录名（Claude 目录式），否则取文件 stem
pub fn skill_id_of(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if file_name.eq_ignore_ascii_case("skill.md") {
        let dir = path.parent()?.file_name()?.to_str()?;
        return Some(dir.to_string());
    }
    path.file_stem()?.to_str().map(String::from)
}

/// 收集解压目录中的技能 md：顶层 `*.md`（排除 README.md）+ `skills/*.md` + 一层子目录的 `*/SKILL.md`
/// （Claude 目录式，id = 目录名，非法目录名跳过并 warn）。三种来源按技能 id 去重后排序。
pub fn collect_skill_files(extracted_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let push_md = |dir: &Path, files: &mut Vec<PathBuf>| {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_md = path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("md");
            if is_md {
                files.push(path);
            }
        }
    };
    // 顶层（排除 README.md，大小写不敏感）
    let mut top: Vec<PathBuf> = Vec::new();
    push_md(extracted_dir, &mut top);
    files.extend(top.into_iter().filter(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| !n.eq_ignore_ascii_case("readme.md"))
            .unwrap_or(false)
    }));
    // skills/ 子目录
    push_md(&extracted_dir.join("skills"), &mut files);
    // Claude 目录式：一层子目录的 */SKILL.md（大小写不敏感）
    if let Ok(entries) = std::fs::read_dir(extracted_dir) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Ok(sub) = std::fs::read_dir(&dir) else { continue };
            for f in sub.flatten() {
                let path = f.path();
                let is_skill_md = path.is_file()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.eq_ignore_ascii_case("skill.md"))
                        .unwrap_or(false);
                if is_skill_md {
                    files.push(path);
                }
            }
        }
    }
    // 按技能 id 去重（先收先生效：顶层 > skills/ > 目录式），非法 id 跳过并 warn
    let mut seen = std::collections::HashSet::new();
    files.retain(|p| match skill_id_of(p) {
        Some(id) if Skill::validate_id(&id).is_ok() => seen.insert(id),
        Some(id) => {
            tracing::warn!(id, path = %p.display(), "跳过非法技能 id（目录式 SKILL.md 的目录名须为 kebab-case）");
            false
        }
        None => false,
    });
    files.sort();
    files
}

/// 安装技能文件到全局技能目录：逐个校验 id + frontmatter 注入 `market: owner/repo`；
/// 同名冲突（预检）或写入失败时回滚本次已写入的文件
pub fn install_skill_files(
    global_dir: &Path,
    market_repo: &str,
    files: &[PathBuf],
) -> Result<Vec<Skill>, DomainError> {
    if files.is_empty() {
        return Err(DomainError::Validation("仓库中没有技能文件".into()));
    }
    // 解析 + 校验全部技能（先解析后写盘，失败零副作用）
    let mut skills: Vec<Skill> = Vec::new();
    for file in files {
        // 目录式 SKILL.md 取目录名为 id，其余取文件 stem
        let id = skill_id_of(file)
            .ok_or_else(|| DomainError::Validation(format!("非法技能文件名：{}", file.display())))?;
        Skill::validate_id(&id)?;
        let text = std::fs::read_to_string(file)
            .map_err(|e| DomainError::Validation(format!("读取技能文件失败：{e}")))?;
        let mut skill = parse_skill(&id, &text, SkillSource::Global)?;
        skill.market_repo = Some(market_repo.to_string());
        skills.push(skill);
    }
    // 同名冲突预检（含目录下已存在但 frontmatter 非法的文件，按文件名判断）
    std::fs::create_dir_all(global_dir)
        .map_err(|e| DomainError::Validation(format!("创建全局技能目录失败：{e}")))?;
    for skill in &skills {
        if global_dir.join(format!("{}.md", skill.id)).exists() {
            return Err(DomainError::Conflict(format!(
                "全局技能已存在同名：{}",
                skill.id
            )));
        }
    }
    // 逐个写入；任一失败回滚本次已写入文件
    let mut written: Vec<PathBuf> = Vec::new();
    for skill in &skills {
        let target = global_dir.join(format!("{}.md", skill.id));
        let text = serialize_skill(
            &skill.name,
            &skill.description,
            skill.enabled,
            skill.market_repo.as_deref(),
            &skill.content,
        );
        if let Err(e) = std::fs::write(&target, text) {
            for f in &written {
                let _ = std::fs::remove_file(f);
            }
            return Err(DomainError::Validation(format!(
                "写入技能文件失败（已回滚 {} 个文件）：{e}",
                written.len()
            )));
        }
        written.push(target);
    }
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_frontmatter() {
        let text = "---\nname: 周报\ndescription: 汇总本周 git 提交\nenabled: true\n---\n正文 $ARGUMENTS 模板\n";
        let s = parse_skill("weekly-report", text, SkillSource::Global).unwrap();
        assert_eq!(s.id, "weekly-report");
        assert_eq!(s.name, "周报");
        assert_eq!(s.description, "汇总本周 git 提交");
        assert!(s.enabled);
        assert_eq!(s.content, "正文 $ARGUMENTS 模板");
        assert_eq!(s.source, SkillSource::Global);
    }

    #[test]
    fn parse_enabled_false_and_unknown_keys() {
        let text = "---\nname: x\nenabled: false\nfoo: bar\n---\nbody";
        let s = parse_skill("a-b", text, SkillSource::Project).unwrap();
        assert!(!s.enabled);
        assert_eq!(s.content, "body");
    }

    #[test]
    fn parse_without_frontmatter_falls_back() {
        let s = parse_skill("plain", "直接就是正文", SkillSource::Global).unwrap();
        assert_eq!(s.name, "plain");
        assert!(s.enabled);
        assert_eq!(s.content, "直接就是正文");
    }

    #[test]
    fn parse_rejects_bad_id() {
        assert!(parse_skill("../evil", "---\nname: x\n---\nb", SkillSource::Global).is_err());
    }

    #[test]
    fn serialize_then_parse_roundtrip() {
        let text = serialize_skill("周报", "描述", false, None, "模板 $ARGUMENTS");
        let s = parse_skill("weekly-report", &text, SkillSource::Global).unwrap();
        assert_eq!(s.name, "周报");
        assert_eq!(s.description, "描述");
        assert!(!s.enabled);
        assert_eq!(s.content, "模板 $ARGUMENTS");
        assert_eq!(s.market_repo, None);
    }

    #[test]
    fn market_key_roundtrip() {
        // 序列化写入 market 键（仅 Some 时）
        let text = serialize_skill("周报", "描述", true, Some("cy/weekly-skills"), "正文");
        assert!(text.contains("market: cy/weekly-skills"));
        let s = parse_skill("weekly-report", &text, SkillSource::Global).unwrap();
        assert_eq!(s.market_repo.as_deref(), Some("cy/weekly-skills"));
        // None 时不写 market 键；解析旧文件回退 None
        let text = serialize_skill("周报", "描述", true, None, "正文");
        assert!(!text.contains("market:"));
        let s = parse_skill("weekly-report", &text, SkillSource::Global).unwrap();
        assert_eq!(s.market_repo, None);
    }

    #[test]
    fn collect_skill_files_excludes_readme_and_includes_skills_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("README.md"), "# readme").unwrap();
        std::fs::write(tmp.path().join("top-skill.md"), "---\nname: T\n---\nb").unwrap();
        std::fs::write(tmp.path().join("notes.txt"), "x").unwrap();
        std::fs::create_dir_all(tmp.path().join("skills")).unwrap();
        std::fs::write(tmp.path().join("skills/inner-skill.md"), "---\nname: I\n---\nb").unwrap();
        let files = collect_skill_files(tmp.path());
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["inner-skill.md", "top-skill.md"], "排除 README、按路径排序");
    }

    #[test]
    fn collect_skill_files_supports_claude_dir_style() {
        let tmp = tempfile::tempdir().unwrap();
        // 目录式：id = 目录名
        std::fs::create_dir_all(tmp.path().join("code-review")).unwrap();
        std::fs::write(tmp.path().join("code-review/SKILL.md"), "---\nname: 评审\n---\nb").unwrap();
        // 大小写不敏感
        std::fs::create_dir_all(tmp.path().join("weekly-report")).unwrap();
        std::fs::write(tmp.path().join("weekly-report/skill.md"), "---\nname: 周报\n---\nb").unwrap();
        // 非法目录名跳过
        std::fs::create_dir_all(tmp.path().join("Bad Name")).unwrap();
        std::fs::write(tmp.path().join("Bad Name/SKILL.md"), "---\nname: 坏\n---\nb").unwrap();
        // 同 id 去重：顶层 dup.md 先收，dup/SKILL.md 被去重
        std::fs::write(tmp.path().join("dup.md"), "---\nname: 顶层版\n---\nb").unwrap();
        std::fs::create_dir_all(tmp.path().join("dup")).unwrap();
        std::fs::write(tmp.path().join("dup/SKILL.md"), "---\nname: 目录版\n---\nb").unwrap();

        let files = collect_skill_files(tmp.path());
        let mut ids: Vec<String> = files.iter().filter_map(|p| skill_id_of(p)).collect();
        ids.sort();
        assert_eq!(
            ids,
            vec!["code-review", "dup", "weekly-report"],
            "目录式收录 + 非法目录名跳过 + 同 id 去重"
        );
        // 去重保留先收的顶层文件
        assert!(files.iter().any(|p| p.ends_with("dup.md")));
        assert!(!files.iter().any(|p| p.ends_with("dup/SKILL.md")));
        // id 推导：目录式取目录名
        let skill_md = files
            .iter()
            .find(|p| p.ends_with("code-review/SKILL.md"))
            .expect("应收录目录式 SKILL.md");
        assert_eq!(skill_id_of(skill_md).as_deref(), Some("code-review"));
    }

    #[test]
    fn install_skill_files_injects_market_and_rolls_back_on_conflict() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("s-one.md"), "---\nname: 一\n---\nbody1").unwrap();
        std::fs::write(repo.path().join("s-two.md"), "---\nname: 二\n---\nbody2").unwrap();
        let global = tempfile::tempdir().unwrap();
        let files = collect_skill_files(repo.path());
        assert_eq!(files.len(), 2);

        // 正常安装：注入 market 键
        let installed = install_skill_files(global.path(), "cy/skills-repo", &files).unwrap();
        assert_eq!(installed.len(), 2);
        let text = std::fs::read_to_string(global.path().join("s-one.md")).unwrap();
        assert!(text.contains("market: cy/skills-repo"));
        assert_eq!(installed[0].market_repo.as_deref(), Some("cy/skills-repo"));
        assert_eq!(installed[0].source, SkillSource::Global);

        // 同名冲突：报错且不产生新写入
        let err = install_skill_files(global.path(), "cy/skills-repo", &files).unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
        assert!(!global.path().join("s-other.md").exists());

        // 空文件列表 → validation
        let err = install_skill_files(global.path(), "cy/x", &[]).unwrap_err();
        assert!(matches!(err, DomainError::Validation(_)));
        assert!(err.to_string().contains("没有技能文件"));
    }

    #[test]
    fn save_scan_delete_project_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let skill = Skill {
            id: "code-review".into(),
            name: "评审".into(),
            description: "d".into(),
            enabled: true,
            source: SkillSource::Project,
            market_repo: None,
            content: "评审 $ARGUMENTS".into(),
        };
        save_skill_file(SkillSource::Project, Some(&root), &skill).unwrap();
        // 文件落在 <项目>/.cyan/skills/ 内
        assert!(tmp.path().join(".cyan/skills/code-review.md").exists());

        let dir = project_skills_dir(&root).unwrap();
        let list = scan_skills(&dir, SkillSource::Project).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "评审");

        delete_skill_file(SkillSource::Project, Some(&root), "code-review").unwrap();
        assert!(scan_skills(&dir, SkillSource::Project).unwrap().is_empty());
        // 幂等
        delete_skill_file(SkillSource::Project, Some(&root), "code-review").unwrap();
    }

    #[test]
    fn save_rejects_id_with_path_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let mut skill = Skill {
            id: "../evil".into(),
            name: "x".into(),
            description: String::new(),
            enabled: true,
            source: SkillSource::Project,
            market_repo: None,
            content: "c".into(),
        };
        // service 层会先 validate；infra 层 resolve 也兜底（id 含非法字符根本过不了 validate_id）
        assert!(Skill::validate_id(&skill.id).is_err());
        skill.id = "ok-skill".into();
        save_skill_file(SkillSource::Project, Some(&root), &skill).unwrap();
        // 确认文件在项目内而非项目外
        assert!(tmp.path().join(".cyan/skills/ok-skill.md").exists());
        assert!(!tmp.path().with_file_name("evil.md").exists());
    }
}
