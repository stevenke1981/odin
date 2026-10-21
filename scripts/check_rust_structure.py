#!/usr/bin/env python3
"""Optional lexical delimiter inspection using Pygments tokens.

NOT a Rust parser, compiler, macro expander or type check. It only lexes each .rs file
and reports unbalanced ([{ and )]} tokens, ignoring string/char literals and ordinary
comments. Pygments mis-tokenizes some Rust macro syntax, so a PASS is weak evidence and
a FAIL may be a lexer artifact; `rust_compilation` is always NOT_RUN here.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / 'verification/lexical-check.json'
PAIRS = {')': '(', ']': '[', '}': '{'}


def counts_as_code(kind, comment, string):
    # Pygments lexes `$(` in macro_rules! and `#[` / `#![` as Comment.Preproc with the
    # delimiter inside the token, so Preproc tokens must still be counted. Other comments
    # and string/char literals are skipped.
    if kind in string:
        return False
    if kind in comment:
        return kind in comment.Preproc
    return True


def check_file(path, lex, lexer, error, comment, string):
    text = path.read_text(encoding='utf-8')
    stack, errors, line = [], [], 1
    for kind, value in lex(text, lexer):
        start = line
        line += value.count('\n')
        if kind in error:
            errors.append('lexer token at line %d: %r' % (start, value))
        if not counts_as_code(kind, comment, string):
            continue
        for char in value:
            if char in '([{':
                stack.append((char, start))
            elif char in PAIRS:
                if not stack or stack.pop()[0] != PAIRS[char]:
                    errors.append('mismatched delimiter: %s at line %d' % (char, start))
    for char, at in stack:
        errors.append('unclosed delimiter: %s opened at line %d' % (char, at))
    return errors


def rust_files(target):
    return [target] if target.is_file() else sorted(target.rglob('*.rs'))


def display(path):
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def main(argv=None):
    parser = argparse.ArgumentParser(description='Lexical delimiter check for Rust sources (not a compiler check).')
    parser.add_argument('--path', type=Path, default=ROOT / 'crates', help='a .rs file or a directory to scan (default: crates/)')
    parser.add_argument('--output', type=Path, default=DEFAULT_OUTPUT, help='JSON report path (default: verification/lexical-check.json)')
    args = parser.parse_args(argv)
    report = {'scope': 'RUST_LEXICAL_DELIMITERS_ONLY', 'rust_compilation': 'NOT_RUN', 'checks': []}
    try:
        from pygments import lex
        from pygments.lexers import RustLexer
        from pygments.token import Comment, Error, String
    except ImportError:
        report.update(status='NOT_RUN', reason='Optional Pygments package unavailable')
    else:
        target = args.path.resolve()
        files = rust_files(target) if target.exists() else []
        if not files:
            report.update(status='FAIL', reason='no .rs files found at ' + str(args.path))
        for path in files:
            errors = check_file(path, lex, RustLexer(), Error, Comment, String)
            report['checks'].append({'path': display(path), 'status': 'FAIL' if errors else 'PASS', 'errors': errors})
        if files:
            report['status'] = 'FAIL' if any(c['status'] == 'FAIL' for c in report['checks']) else 'PASS'
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'scope': report['scope'], 'status': report['status'], 'files': len(report['checks'])}))
    return 1 if report['status'] == 'FAIL' else 0


if __name__ == '__main__':
    raise SystemExit(main())
