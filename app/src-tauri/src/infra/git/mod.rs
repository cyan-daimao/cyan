//! git checkpoint：git2 实现。写文件前将「变更前内容」存为 git blob，回滚即写回 blob 内容。
//! 非 git 项目：先 `git init` + 空提交基线（TECH_DESIGN 6.3）。

use std::path::Path;

use crate::domain::agent::CheckpointGateway;

/// 确保项目存在 git 仓库（没有则 init 并建空提交基线）
pub fn ensure_repo(root: &Path) -> anyhow::Result<git2::Repository> {
    match git2::Repository::open(root) {
        Ok(repo) => Ok(repo),
        Err(_) => {
            let repo = git2::Repository::init(root)?;
            // 空提交基线，保证后续树引用操作有 HEAD 可依
            let sig = git2::Signature::now("cyan", "cyan@local")?;
            let tree_id = {
                let mut index = repo.index()?;
                index.write_tree()?
            };
            {
                let tree = repo.find_tree(tree_id)?;
                repo.commit(Some("HEAD"), &sig, &sig, "chore: cyan baseline", &tree, &[])?;
            }
            Ok(repo)
        }
    }
}

/// 打 checkpoint：把变更前内容写入 git blob，返回 blob oid（作为 cyan_checkpoint.git_ref）。
/// `before` 为 None（新建文件）时存空 blob。
pub fn checkpoint(root: &Path, before: Option<&[u8]>) -> anyhow::Result<String> {
    let repo = ensure_repo(root)?;
    let oid = repo.blob(before.unwrap_or(&[]))?;
    Ok(oid.to_string())
}

/// 行级 diff 统计（多重集差）：返回 (新增行数, 删除行数)
pub fn diff_lines(before: &str, after: &str) -> (i64, i64) {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for line in before.lines() {
        *counts.entry(line).or_default() += 1;
    }
    for line in after.lines() {
        *counts.entry(line).or_default() -= 1;
    }
    let mut add = 0i64;
    let mut del = 0i64;
    for v in counts.values() {
        if *v > 0 {
            del += *v;
        } else {
            add += -*v;
        }
    }
    (add, del)
}

/// checkpoint 回滚实现（domain CheckpointGateway 端口）
pub struct GitCheckpointGateway;

impl CheckpointGateway for GitCheckpointGateway {
    fn rollback(&self, project_root: &Path, git_ref: &str, rel_path: &str) -> anyhow::Result<()> {
        let repo = git2::Repository::open(project_root)?;
        let oid = git2::Oid::from_str(git_ref)?;
        let blob = repo.find_blob(oid)?;
        let target = project_root.join(rel_path);
        // 空 blob 且文件在 checkpoint 前不存在 → 回滚为删除文件
        if blob.content().is_empty() {
            if target.exists() {
                std::fs::remove_file(&target)?;
            }
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, blob.content())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_lines_multiset() {
        let (add, del) = diff_lines("a\nb\nc\n", "a\nc\nd\n");
        assert_eq!((add, del), (1, 1));
        let (add, del) = diff_lines("", "x\ny\n");
        assert_eq!((add, del), (2, 0));
    }

    #[test]
    fn checkpoint_and_rollback_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("a.txt"), "v1\n").unwrap();

        // 变更前打 checkpoint
        let before = std::fs::read(root.join("a.txt")).unwrap();
        let git_ref = checkpoint(root, Some(&before)).unwrap();

        // 修改后回滚
        std::fs::write(root.join("a.txt"), "v2\n").unwrap();
        GitCheckpointGateway.rollback(root, &git_ref, "a.txt").unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v1\n");

        // 新建文件（before=None → 空 blob）回滚即删除
        let new_ref = checkpoint(root, None).unwrap();
        std::fs::write(root.join("new.txt"), "hello\n").unwrap();
        GitCheckpointGateway.rollback(root, &new_ref, "new.txt").unwrap();
        assert!(!root.join("new.txt").exists());
    }
}
