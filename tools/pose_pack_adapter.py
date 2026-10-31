#!/usr/bin/env python3
"""姿势库适配器：把第三方白模库的元数据转成显影的 pose pack。

不做下载、不打包、不改源文件。只生成：

    <out>/pack.json
    <out>/catalog.json

图片默认**不复制**：catalog 里写相对 `ComfyUI/input` 的路径（后端认识带 `/` 的
路径，会直接读）。源不在 `input` 下时用 `--copy` 复制到 `<out>/line/`。

用法（ai-pose-gallery-zh 为例）：
    # 1) 先确认 LICENSE 允许本地使用/再分发
    # 2) 把仓库放到 ComfyUI/input/ai-pose-gallery-zh（或任意目录 + --copy）
    python tools/pose_pack_adapter.py \
        --src "D:/.../ComfyUI/input/ai-pose-gallery-zh" \
        --out "D:/.../ComfyUI/input/pose_packs/ai-pose" \
        --input-root "D:/.../ComfyUI/input" \
        --name "ai-pose 中文库"

    # 只预览不写盘
    python tools/pose_pack_adapter.py --src ... --out ... --dry-run

    # 自检（不碰真实数据）
    python tools/pose_pack_adapter.py --selftest
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path

VIEW_ALIASES = {
    "front": "front", "正面": "front", "正视图": "front",
    "side": "side", "侧面": "side", "侧视图": "side",
    "diag": "diag", "斜侧": "diag", "斜侧视": "diag", "threequarter": "diag", "three_quarter": "diag",
    "above": "above", "俯视": "above", "俯视图": "above", "top": "above",
    "below": "below", "仰视": "below", "仰视图": "below", "bottom": "below",
}
NUM_VIEWS = {"1": "front", "2": "diag", "3": "side", "4": "above", "5": "below"}
IMAGE_EXT = {".webp", ".png", ".jpg", ".jpeg", ".avif"}


def angle_of(text: str) -> str | None:
    t = str(text or "").lower().replace("-", "_").replace(" ", "_")
    for key, val in VIEW_ALIASES.items():
        if key in t:
            return val
    m = re.search(r"(?:^|[_-])([1-5])(?:$|[_\-.])", t)
    if m:
        return NUM_VIEWS[m.group(1)]
    return None


def records_of(data):
    if isinstance(data, list):
        return [x for x in data if isinstance(x, dict)]
    if isinstance(data, dict):
        for key in ("poses", "items", "data", "list", "records", "entries"):
            v = data.get(key)
            if isinstance(v, list):
                return [x for x in v if isinstance(x, dict)]
        vals = [v for v in data.values() if isinstance(v, dict)]
        if vals:
            out = []
            for k, v in data.items():
                if isinstance(v, dict):
                    v = dict(v)
                    v.setdefault("id", k)
                    out.append(v)
            return out
    return []


def find_metadata(src: Path, prefer: tuple[str, ...] | None = None) -> tuple[Path | None, list]:
    if prefer:
        for name in prefer:
            for p in sorted(src.rglob(name)):
                try:
                    data = json.loads(p.read_text(encoding="utf-8", errors="ignore"))
                except Exception:
                    continue
                recs = records_of(data)
                if recs:
                    return p, recs
    best: tuple[int, Path | None, list] = (0, None, [])
    for p in sorted(src.rglob("*.json")):
        if p.stat().st_size > 80 * 1024 * 1024:
            continue
        try:
            data = json.loads(p.read_text(encoding="utf-8", errors="ignore"))
        except Exception:
            continue
        recs = records_of(data)
        if len(recs) > best[0]:
            best = (len(recs), p, recs)
    return best[1], best[2]


def pick(d: dict, *keys, default=""):
    for k in keys:
        if k in d and d[k] not in (None, "", []):
            return d[k]
    return default




AI_ANGLES = {
    "正面": "front", "侧面": "side", "斜侧": "diag", "俯视": "above", "仰视": "below",
    "上から": "above", "下から": "below",
}
AI_SYSTEMS = {
    "上半身单人": "upper",
    "全身单人・性感系": "full_sexy",
    "全身单人・标准系": "full_std",
    "双人缠绵": "duo",
}


def lexical_abs(p) -> Path:
    """不解析 junction/软链接的绝对路径，保证还能相对 input 计算。"""
    return Path(os.path.abspath(str(p)))


def rel_to_input(p: Path, input_root: Path) -> str | None:
    try:
        return lexical_abs(p).relative_to(lexical_abs(input_root)).as_posix()
    except ValueError:
        return None


def is_ai_pose(records: list) -> bool:
    return any(("姿势编号" in r or "ポーズ番号" in r) for r in records)


def ai_pose_convert(src: Path, input_root: Path, records: list, out: Path, copy_images: bool):
    """ai-pose-gallery-zh：一行一个机位，按 体系+编号 聚合成一组。"""
    groups: dict[tuple[str, str], list] = {}
    for rec in records:
        system = str(pick(rec, "体系", "系統", default=""))
        num = str(pick(rec, "姿势编号", "ポーズ番号", default=""))
        if not num:
            continue
        groups.setdefault((system, num), []).append(rec)

    poses = []
    copied = 0
    for (system, num), recs in groups.items():
        first = recs[0]
        name = str(pick(first, "姿势名称", "ポーズ名", default=num))
        cls = str(pick(first, "分类", "分類", default=system))
        views: dict[str, str] = {}
        thumbs: dict[str, str] = {}
        for rec in recs:
            angle = AI_ANGLES.get(str(pick(rec, "角度", "アングル", default="")))
            if not angle:
                continue
            raw = str(pick(rec, "原图", "原寸", default=""))
            if not raw:
                continue
            p = lexical_abs(src / raw)
            if not p.is_file():
                continue
            if copy_images:
                d = out / "line"
                d.mkdir(parents=True, exist_ok=True)
                dest = d / p.name
                if not dest.exists():
                    shutil.copy2(p, dest)
                    copied += 1
                views[angle] = p.name
            else:
                rel = rel_to_input(p, input_root)
                if rel:
                    views[angle] = rel
            thumb_raw = str(pick(rec, "缩略图", "サムネ", default=""))
            if thumb_raw and not copy_images:
                tp = lexical_abs(src / thumb_raw)
                trel = rel_to_input(tp, input_root) if tp.is_file() else None
                if trel:
                    thumbs[angle] = trel
        if not views:
            continue
        slug = AI_SYSTEMS.get(system) or re.sub(r"[^a-z0-9]+", "_", system.lower()).strip("_") or "pose"
        item = {
            "id": f"{slug}_{num}",
            "name_cn": name,
            "name_en": f"{slug}_{num}",
            "class_cn": cls,
            "desc_en": name,
            "views": views,
        }
        if thumbs:
            item["thumbs"] = thumbs
        poses.append(item)
    return poses, copied


def write_pack(out: Path, pack_name: str, poses: list, copied: int, dry_run: bool):
    if dry_run:
        print(f"[dry-run] 姿势：{len(poses)}，图片：{sum(len(p['views']) for p in poses)}（未写盘）")
        return
    out.mkdir(parents=True, exist_ok=True)
    (out / "pack.json").write_text(
        json.dumps({"name": pack_name}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    (out / "catalog.json").write_text(
        json.dumps(poses, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"写出：{out}")
    print(f"姿势：{len(poses)}，视图：{sum(len(p['views']) for p in poses)}，复制图片：{copied}")
    if not copied:
        print("提示：catalog 里是相对 ComfyUI/input 的图片路径；源必须留在 input 下。")


def image_files(src: Path) -> list[Path]:
    return [p for p in src.rglob("*") if p.is_file() and p.suffix.lower() in IMAGE_EXT]


def view_sources(record: dict):
    raw = pick(record, "views", "images", "angles", "files", default={})
    out: list[tuple[str | None, str]] = []
    if isinstance(raw, dict):
        for k, v in raw.items():
            if isinstance(v, str):
                out.append((angle_of(k) or angle_of(v), v))
            elif isinstance(v, dict):
                path = pick(v, "file", "path", "src", "image", "url", default="")
                if path:
                    out.append((angle_of(k) or angle_of(path), path))
    elif isinstance(raw, list):
        for v in raw:
            if isinstance(v, str):
                out.append((angle_of(v), v))
            elif isinstance(v, dict):
                path = pick(v, "file", "path", "src", "image", "url", default="")
                if path:
                    out.append((angle_of(pick(v, "angle", "view", "name", default="")) or angle_of(path), path))
    return out


def convert(src: Path, out: Path, input_root: Path, pack_name: str, copy_images: bool, dry_run: bool, profile: str = "auto"):
    prefer = ("pose-zh.json", "pose.json") if profile in ("auto", "ai-pose") else None
    meta_path, records = find_metadata(src, prefer)
    if not records:
        # 没有元数据：按文件名兜底
        records = []
        for p in image_files(src):
            records.append({"id": p.stem, "name_cn": p.stem, "views": {"auto": p.name}})

    if profile in ("auto", "ai-pose") and is_ai_pose(records):
        poses, copied = ai_pose_convert(src, input_root, records, out, copy_images)
        if dry_run:
            print(f"[dry-run] 元数据：{meta_path}")
        write_pack(out, pack_name, poses, copied, dry_run)
        return poses

    files = image_files(src)
    by_name = {p.name.lower(): p for p in files}
    by_stem = {p.stem.lower(): p for p in files}
    used = set()
    poses = []
    copied = 0

    for rec in records:
        pid = str(pick(rec, "id", "key", "code", "pose_id", "slug", "name_en", "name", default="")).strip()
        if not pid:
            continue
        name_cn = str(pick(rec, "name_cn", "name_zh", "cn_name", "title_cn", "title", "name", default=pid))
        name_en = str(pick(rec, "name_en", "en_name", "english", "name", default=""))
        cls = str(pick(rec, "class_cn", "category", "class", "type", "tag", "group", default=""))
        desc = str(pick(rec, "desc_en", "description_en", "description", "prompt", "desc", default=""))
        views: dict[str, str] = {}

        for angle, raw in view_sources(rec):
            if not raw or "://" in raw:
                continue
            p = Path(raw)
            if not p.is_absolute():
                p = lexical_abs(src / raw)
            if not p.is_file():
                p = by_name.get(Path(raw).name.lower()) or by_stem.get(Path(raw).stem.lower())
            if not p or not Path(p).is_file():
                continue
            p = lexical_abs(p)
            used.add(p)
            if copy_images:
                dest_dir = out / "line"
                if not dry_run:
                    dest_dir.mkdir(parents=True, exist_ok=True)
                    dest = dest_dir / p.name
                    if not dest.exists():
                        shutil.copy2(p, dest)
                        copied += 1
                views.setdefault(angle or "front", p.name)
            else:
                try:
                    rel = p.relative_to(lexical_abs(input_root))
                except ValueError:
                    continue
                views.setdefault(angle or "front", rel.as_posix())

        if not views:
            # 按 id 在文件名里找
            for p in files:
                if p in used:
                    continue
                if pid.lower() in p.stem.lower():
                    angle = angle_of(p.stem) or "front"
                    if copy_images:
                        dest_dir = out / "line"
                        if not dry_run:
                            dest_dir.mkdir(parents=True, exist_ok=True)
                            dest = dest_dir / p.name
                            if not dest.exists():
                                shutil.copy2(p, dest)
                                copied += 1
                        views.setdefault(angle, p.name)
                    else:
                        try:
                            views.setdefault(angle, p.relative_to(lexical_abs(input_root)).as_posix())
                        except ValueError:
                            pass
        if not views:
            continue
        poses.append({
            "id": pid,
            "name_cn": name_cn,
            "name_en": name_en,
            "class_cn": cls,
            "desc_en": desc,
            "views": views,
        })

    if not dry_run:
        print(f"元数据：{meta_path or '无（按文件名）'}")
    write_pack(out, pack_name, poses, copied, dry_run)
    return poses


def selftest():
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        input_root = root / "input"
        src = input_root / "ai-src"
        (src / "images").mkdir(parents=True)
        (src / "images" / "P1_front.webp").write_bytes(b"x")
        (src / "images" / "P1_side.webp").write_bytes(b"x")
        (src / "images" / "P2_1.webp").write_bytes(b"x")
        (src / "meta.json").write_text(json.dumps([
            {"id": "P1", "name_cn": "坐姿", "class_cn": "坐", "desc_en": "sitting",
             "views": {"front": "images/P1_front.webp", "side": "images/P1_side.webp"}},
            {"id": "P2", "name": "站姿", "images": [{"view": "front", "file": "images/P2_1.webp"}]},
        ], ensure_ascii=False), encoding="utf-8")
        out = input_root / "pose_packs" / "test"
        poses = convert(src, out, input_root, "测试包", copy_images=False, dry_run=False, profile="generic")
        assert len(poses) == 2, poses
        assert poses[0]["views"]["front"] == "ai-src/images/P1_front.webp"
        assert poses[1]["views"]["front"] == "ai-src/images/P2_1.webp"
        assert json.loads((out / "pack.json").read_text(encoding="utf-8"))["name"] == "测试包"
    print("selftest ok")


def main():
    ap = argparse.ArgumentParser(description="第三方姿势库 → 显影 pose pack")
    ap.add_argument("--src", type=Path, help="第三方库目录（含元数据/图片）")
    ap.add_argument("--out", type=Path, help="输出 pack 目录（.../input/pose_packs/<id>）")
    ap.add_argument("--input-root", type=Path, default=None, help="ComfyUI/input；默认从 --out 推断")
    ap.add_argument("--name", default="", help="显示名；默认用目录名")
    ap.add_argument("--copy", action="store_true", help="把图片复制到 <out>/line（源在 input 外时必须）")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--profile", choices=["auto", "generic", "ai-pose"], default="auto")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        selftest()
        return
    if not args.src or not args.out:
        ap.error("需要 --src 和 --out（或 --selftest）")

    src = lexical_abs(args.src)
    out = lexical_abs(args.out)
    input_root = lexical_abs(args.input_root) if args.input_root else out.parent.parent
    if not src.is_dir():
        print(f"源目录不存在：{src}", file=sys.stderr)
        sys.exit(1)
    if not args.copy:
        try:
            src.relative_to(input_root)
        except ValueError:
            print("源不在 ComfyUI/input 下：请加 --copy，或把库放到 input 里再跑。", file=sys.stderr)
            sys.exit(1)
    name = args.name or out.name
    convert(src, out, input_root, name, args.copy, args.dry_run, args.profile)


if __name__ == "__main__":
    main()
