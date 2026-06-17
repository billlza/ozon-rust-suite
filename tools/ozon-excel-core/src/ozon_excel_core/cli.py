"""argparse CLI: process / verify / gen-sample.

Exit codes:
  0  success (process ok; verify ok)
  1  verify found unexpected (frozen/header/structural) changes
  2  config/mapping error, or --in == --out
  3  preflight risk under error policy
"""

from __future__ import annotations

import argparse
import json
import os
import sys

from .config import load_config
from .errors import ConfigError, MappingError, PreflightError, TransformError


def _abs(p):
    return os.path.abspath(str(p))


def main(argv=None) -> int:
    # Parent parser holding global flags so they're accepted both before AND
    # after the subcommand (e.g. `process ... --quiet`).
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--quiet", action="store_true", help="suppress non-essential output")
    common.add_argument("--verbose", action="store_true", help="extra detail")

    parser = argparse.ArgumentParser(
        prog="ozon-excel-core",
        description="Surgical Ozon .xlsx content rewriter + verifier (pure core, no network).",
        parents=[common],
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_proc = sub.add_parser("process", parents=[common],
                            help="rewrite content columns into a new file")
    p_proc.add_argument("--in", dest="in_path", required=True)
    p_proc.add_argument("--out", dest="out_path", required=True)
    p_proc.add_argument("--config", required=True)
    p_proc.add_argument("--transform", default="identity",
                        help="identity | example | pkg.module:Factory")
    p_proc.add_argument("--on-risk", choices=["error", "warn"], default=None,
                        help="override policy.on_preflight_risk")
    p_proc.add_argument("--verify", action="store_true",
                        help="run the verifier after writing and adopt its exit code")

    p_ver = sub.add_parser("verify", parents=[common],
                           help="prove only mapped columns changed")
    p_ver.add_argument("--in", dest="in_path", required=True)
    p_ver.add_argument("--out", dest="out_path", required=True)
    p_ver.add_argument("--config", required=True)
    p_ver.add_argument("--report", choices=["text", "json"], default="text")

    p_gen = sub.add_parser("gen-sample", parents=[common],
                           help="write a representative Ozon-style .xlsx")
    p_gen.add_argument("--out", dest="out_path", required=True)

    p_push = sub.add_parser(
        "push-ozon", parents=[common],
        help="push a relisted .xlsx back to Ozon (images, optional title)")
    p_push.add_argument("--in", dest="in_path", required=True)
    p_push.add_argument("--config", required=True)
    p_push.add_argument("--sheet", default=None)
    p_push.add_argument("--offer-col", dest="offer_col", default=None)
    p_push.add_argument("--keep-existing", dest="keep_existing", action="store_true")
    p_push.add_argument("--push-title", dest="push_title", action="store_true")
    push_grp = p_push.add_mutually_exclusive_group()
    push_grp.add_argument("--dry-run", dest="apply", action="store_false", default=False)
    push_grp.add_argument("--apply", dest="apply", action="store_true")
    p_push.add_argument("--limit", type=int, default=1)
    p_push.add_argument("--poll", type=int, default=1)

    args = parser.parse_args(argv)

    try:
        if args.command == "process":
            return _cmd_process(args)
        if args.command == "verify":
            return _cmd_verify(args)
        if args.command == "gen-sample":
            return _cmd_gen_sample(args)
        if args.command == "push-ozon":
            return _cmd_push_ozon(args)
    except (ConfigError, MappingError, TransformError) as exc:
        print(f"ERROR (config/mapping): {exc}", file=sys.stderr)
        return 2
    except PreflightError as exc:
        print(f"ERROR (preflight risk): {exc}", file=sys.stderr)
        return 3
    return 2


def _cmd_process(args) -> int:
    from . import writer
    from .transforms import get_transform

    if _abs(args.in_path) == _abs(args.out_path):
        print("ERROR: --in and --out must differ", file=sys.stderr)
        return 2

    config = load_config(args.config)
    if args.on_risk is not None:
        from dataclasses import replace
        config = replace(config, policy=replace(config.policy, on_preflight_risk=args.on_risk))

    transform = get_transform(args.transform)
    result = writer.process(args.in_path, args.out_path, config, transform)

    if not args.quiet:
        print("=== ozon-excel-core process ===")
        print(f"in  : {_abs(result.in_path)}")
        print(f"out : {_abs(result.out_path)}")
        print(f"transform: {getattr(transform, 'name', args.transform)}")
        for line in result.mapping_summary:
            print(f"map : {line}")
        print(f"rows seen: {result.rows_seen}")
        print(f"cells changed: {result.total_changed()}")
        for role, n in sorted(result.changed_by_role.items()):
            print(f"  {role}: {n}")
        if result.skipped_embedded:
            print(f"skipped embedded-image cells: {len(result.skipped_embedded)}")
        if result.skipped_merged:
            print(f"skipped merged non-anchor cells: {len(result.skipped_merged)}")
        for risk in result.preflight_warnings:
            print(f"preflight WARN [{risk.severity}] {risk.sheet or '<wb>'}: {risk.detail}")

    if args.verify:
        from . import verifier

        report = verifier.verify(args.in_path, args.out_path, config)
        if not args.quiet:
            print()
            print(report.to_text())
        return 0 if report.ok else 1

    return 0


def _cmd_verify(args) -> int:
    from . import verifier

    config = load_config(args.config)
    report = verifier.verify(args.in_path, args.out_path, config)

    if args.report == "json":
        print(json.dumps(report.to_json_dict(), ensure_ascii=False, indent=2))
    else:
        print(report.to_text())

    return 0 if report.ok else 1


def _cmd_gen_sample(args) -> int:
    from .sample import gen_sample

    gen_sample(args.out_path)
    if not args.quiet:
        print(f"wrote sample workbook: {_abs(args.out_path)}")
    return 0


def _load_push_module():
    """Load scripts/push_ozon.py (it lives outside the package, alongside the
    other scripts) so the `push-ozon` subcommand shares its logic."""
    import importlib.util

    here = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    path = os.path.join(here, "scripts", "push_ozon.py")
    spec = importlib.util.spec_from_file_location("push_ozon", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _cmd_push_ozon(args) -> int:
    push = _load_push_module()
    argv = ["--in", args.in_path, "--config", args.config, "--limit", str(args.limit),
            "--poll", str(args.poll)]
    if args.sheet:
        argv += ["--sheet", args.sheet]
    if args.offer_col:
        argv += ["--offer-col", args.offer_col]
    if args.keep_existing:
        argv.append("--keep-existing")
    if args.push_title:
        argv.append("--push-title")
    argv.append("--apply" if args.apply else "--dry-run")
    return push.main(argv)


if __name__ == "__main__":
    raise SystemExit(main())
