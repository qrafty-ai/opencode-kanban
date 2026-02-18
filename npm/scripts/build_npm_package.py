#!/usr/bin/env python3

import argparse
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NPM_ROOT = ROOT / "npm"
PACKAGE_TEMPLATE_PATH = NPM_ROOT / "package.json"
BIN_SOURCE_PATH = NPM_ROOT / "bin" / "opencode-kanban.js"
PACKAGE_NAME = "@qrafty-ai/opencode-kanban"


def package_name_to_filename(name: str) -> str:
    return name.replace("@", "").replace("/", "-")


PACKAGE_FILENAME = package_name_to_filename(PACKAGE_NAME)

PLATFORM_PACKAGES: dict[str, dict[str, str]] = {
    "linux-x64": {
        "target": "x86_64-unknown-linux-gnu",
        "os": "linux",
        "cpu": "x64",
    },
    "darwin-arm64": {
        "target": "aarch64-apple-darwin",
        "os": "darwin",
        "cpu": "arm64",
    },
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--vendor-src", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    return parser.parse_args()


def write_json(path: Path, payload: dict[str, object]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2)
        handle.write("\n")


def run_npm_pack(staging_dir: Path, out_dir: Path, output_name: str) -> None:
    stdout = subprocess.check_output(
        ["npm", "pack", "--json", "--pack-destination", str(out_dir.resolve())],
        cwd=staging_dir,
        text=True,
    )
    pack_result = json.loads(stdout)
    if not pack_result:
        raise RuntimeError("npm pack did not return an output tarball")

    generated_name = pack_result[0].get("filename")
    if not isinstance(generated_name, str):
        raise RuntimeError("npm pack output did not include a filename")

    generated_path = out_dir / generated_name
    if not generated_path.exists():
        raise RuntimeError(f"Expected npm tarball not found: {generated_path}")

    final_path = out_dir / output_name
    if final_path.exists():
        final_path.unlink()
    generated_path.rename(final_path)


def load_package_template() -> dict[str, object]:
    with PACKAGE_TEMPLATE_PATH.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def stage_main_package(
    version: str,
    vendor_src: Path,
    out_dir: Path,
    package_template: dict[str, object],
    temp_root: Path,
) -> None:
    staging_dir = temp_root / "main"
    (staging_dir / "bin").mkdir(parents=True, exist_ok=True)

    shutil.copy2(BIN_SOURCE_PATH, staging_dir / "bin" / "opencode-kanban.js")

    readme_src = ROOT / "README.md"
    if readme_src.exists():
        shutil.copy2(readme_src, staging_dir / "README.md")

    license_src = ROOT / "LICENSE"
    if license_src.exists():
        shutil.copy2(license_src, staging_dir / "LICENSE")

    package_json = dict(package_template)
    package_json["name"] = PACKAGE_NAME
    package_json["version"] = version
    package_json["files"] = ["bin"]
    package_json["optionalDependencies"] = {
        f"{PACKAGE_NAME}-{platform_tag}": f"npm:{PACKAGE_NAME}@{version}-{platform_tag}"
        for platform_tag in PLATFORM_PACKAGES
    }

    write_json(staging_dir / "package.json", package_json)

    run_npm_pack(
        staging_dir=staging_dir,
        out_dir=out_dir,
        output_name=f"{PACKAGE_FILENAME}-npm-{version}.tgz",
    )

    for platform in PLATFORM_PACKAGES.values():
        target_dir = vendor_src / platform["target"]
        if not target_dir.exists():
            raise RuntimeError(
                f"Missing vendor payload for target: {platform['target']}"
            )


def stage_platform_package(
    version: str,
    platform_tag: str,
    vendor_src: Path,
    out_dir: Path,
    package_template: dict[str, object],
    temp_root: Path,
) -> None:
    platform_config = PLATFORM_PACKAGES[platform_tag]
    target = platform_config["target"]

    binary_path = vendor_src / target / "opencode-kanban" / "opencode-kanban"
    if not binary_path.exists():
        raise RuntimeError(f"Missing binary for target {target}: {binary_path}")

    staging_dir = temp_root / platform_tag
    destination_vendor_root = staging_dir / "vendor" / target
    destination_vendor_root.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(vendor_src / target, destination_vendor_root)

    staged_binary_path = destination_vendor_root / "opencode-kanban" / "opencode-kanban"
    if not staged_binary_path.exists():
        raise RuntimeError(
            f"Missing staged binary for target {target}: {staged_binary_path}"
        )
    staged_binary_path.chmod(0o755)

    readme_src = ROOT / "README.md"
    if readme_src.exists():
        shutil.copy2(readme_src, staging_dir / "README.md")

    license_src = ROOT / "LICENSE"
    if license_src.exists():
        shutil.copy2(license_src, staging_dir / "LICENSE")

    package_json = {
        "name": PACKAGE_NAME,
        "version": f"{version}-{platform_tag}",
        "description": package_template.get("description", ""),
        "license": package_template.get("license", "MIT"),
        "os": [platform_config["os"]],
        "cpu": [platform_config["cpu"]],
        "files": ["vendor"],
    }

    repository = package_template.get("repository")
    if isinstance(repository, dict):
        package_json["repository"] = repository

    engines = package_template.get("engines")
    if isinstance(engines, dict):
        package_json["engines"] = engines

    write_json(staging_dir / "package.json", package_json)
    run_npm_pack(
        staging_dir=staging_dir,
        out_dir=out_dir,
        output_name=f"{PACKAGE_FILENAME}-npm-{platform_tag}-{version}.tgz",
    )


def main() -> int:
    args = parse_args()
    version = args.version.strip()
    vendor_src = args.vendor_src.resolve()
    out_dir = args.out_dir.resolve()

    if not vendor_src.exists():
        raise RuntimeError(f"Vendor source path does not exist: {vendor_src}")

    out_dir.mkdir(parents=True, exist_ok=True)
    package_template = load_package_template()

    with tempfile.TemporaryDirectory(prefix="opencode-kanban-npm-stage-") as temp_dir:
        temp_root = Path(temp_dir)

        for platform_tag in PLATFORM_PACKAGES:
            stage_platform_package(
                version=version,
                platform_tag=platform_tag,
                vendor_src=vendor_src,
                out_dir=out_dir,
                package_template=package_template,
                temp_root=temp_root,
            )

        stage_main_package(
            version=version,
            vendor_src=vendor_src,
            out_dir=out_dir,
            package_template=package_template,
            temp_root=temp_root,
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
