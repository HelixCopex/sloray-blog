#!/usr/bin/env python3
"""校验 AGENTS.md 约定的依赖方向、workspace 引用与 unsafe 禁令。"""
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
ALLOWED = {
    "sloray-media": {"sloray-config", "sloray-store"},
    "sloray-content": {"sloray-store"},
    "sloray-config": set(),
    "sloray-store": {"sloray-config"},
    "sloray-auth": {"sloray-store"},
    "sloray-setup": {"sloray-config", "sloray-auth", "sloray-migration"},
    "sloray-migration": set(),
    "sloray-web": {"sloray-config", "sloray-store", "sloray-auth", "sloray-setup", "sloray-migration", "sloray-content", "sloray-media"},
}


def check(root=ROOT):
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    workspace = manifest["workspace"]
    version = workspace["package"]["version"]
    deps = workspace["dependencies"]
    errors = []
    for name, value in deps.items():
        if isinstance(value, dict) and "path" in value and value.get("version") != version:
            errors.append(f"{name}: path 依赖 version 必须为 {version}")
    for path in [root / "Cargo.toml"] + [root / m / "Cargo.toml" for m in workspace["members"]]:
        data = tomllib.loads(path.read_text())
        name = data["package"]["name"]
        if name != "sloray" and name not in ALLOWED:
            errors.append(f"{name}: 新 crate 必须在门禁中声明职责与允许依赖")
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            for dep, value in data.get(section, {}).items():
                if not isinstance(value, dict) or value.get("workspace") is not True:
                    errors.append(f"{name}: {dep} 必须使用 workspace = true")
                internal = deps.get(dep, {})
                if isinstance(internal, dict) and "path" in internal:
                    if name != "sloray" and dep not in ALLOWED.get(name, set()):
                        errors.append(f"{name} → {dep}: 违反 crate 依赖方向")
        source = path.parent / ("src/main.rs" if name == "sloray" else "src/lib.rs")
        if "#![forbid(unsafe_code)]" not in source.read_text():
            errors.append(f"{name}: crate 入口缺少 forbid(unsafe_code)")
    return errors


if __name__ == "__main__":
    failures = check()
    for failure in failures:
        print(f"  ✗ {failure}", file=sys.stderr)
    if failures:
        sys.exit(1)
    print("  ✓ crate 依赖方向 / workspace 版本引用 / unsafe 禁令通过")
