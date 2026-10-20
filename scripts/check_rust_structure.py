#!/usr/bin/env python3
"""Optional lexical delimiter inspection. NOT a Rust parser, compiler or type check."""
from pathlib import Path
import json
ROOT = Path(__file__).resolve().parents[1]
def main():
    report = {'scope': 'RUST_LEXICAL_DELIMITERS_ONLY', 'rust_compilation': 'NOT_RUN', 'checks': []}
    try:
        from pygments import lex
        from pygments.lexers import RustLexer
        from pygments.token import Punctuation, Error
    except ImportError:
        report.update(status='NOT_RUN', reason='Optional Pygments package unavailable')
    else:
        for path in sorted((ROOT / 'crates').rglob('*.rs')):
            stack = []
            errors = []
            for kind, value in lex(path.read_text(encoding='utf-8'), RustLexer()):
                if kind in Error:
                    errors.append('lexer token: ' + repr(value))
                if kind in Punctuation:
                    for char in value:
                        if char in '([{':
                            stack.append(char)
                        elif char in ')]}':
                            if not stack or stack.pop() != {')': '(', ']': '[', '}': '{'}[char]:
                                errors.append('mismatched delimiter: ' + char)
            if stack:
                errors.append('unclosed delimiters: ' + ''.join(stack))
            report['checks'].append({'path': str(path.relative_to(ROOT)), 'status': 'FAIL' if errors else 'PASS', 'errors': errors})
        report['status'] = 'FAIL' if any(c['status'] == 'FAIL' for c in report['checks']) else 'PASS'
    output = ROOT / 'verification/lexical-check.json'
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'scope': report['scope'], 'status': report['status'], 'files': len(report['checks'])}))
    return 1 if report['status'] == 'FAIL' else 0
if __name__ == '__main__':
    raise SystemExit(main())
