"""工作区子目录场景下的 Git 相对路径换算。"""

from __future__ import annotations


def normalize_relative(path: str) -> str:
    """返回安全、正斜杠分隔的相对路径；越界路径返回空串。"""
    raw = str(path or "").replace("\\", "/").strip().strip("/")
    if not raw or raw == ".":
        return ""
    parts: list[str] = []
    for part in raw.split("/"):
        if not part or part == ".":
            continue
        if part == "..":
            return ""
        parts.append(part)
    return "/".join(parts)


def review_path_to_editor_path(review_path: str, real_path: str, git_root: str) -> str | None:
    """将 Git 根相对路径换成当前打开子工作区的相对路径。"""
    path = normalize_relative(review_path)
    prefix = _workspace_prefix(real_path, git_root)
    if not path or prefix is None:
        return None
    if not prefix:
        return path
    if path.startswith(prefix + "/"):
        return path[len(prefix) + 1 :]
    return None


def editor_path_to_review_path(editor_path: str, real_path: str, git_root: str) -> str | None:
    """将当前子工作区相对路径换回 Git 根相对路径。"""
    path = normalize_relative(editor_path)
    prefix = _workspace_prefix(real_path, git_root)
    if not path or prefix is None:
        return None
    return f"{prefix}/{path}" if prefix else path


def _workspace_prefix(real_path: str, git_root: str) -> str | None:
    real = _normalize_absolute(real_path)
    root = _normalize_absolute(git_root)
    if not real or not root:
        return None
    if real.casefold() == root.casefold():
        return ""
    root_with_sep = root.rstrip("/") + "/"
    if not real.casefold().startswith(root_with_sep.casefold()):
        return None
    return normalize_relative(real[len(root_with_sep) :])


def _normalize_absolute(path: str) -> str:
    return str(path or "").replace("\\", "/").rstrip("/")
