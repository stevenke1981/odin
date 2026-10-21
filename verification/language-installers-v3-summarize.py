"""Assemble source-bound delivery documentation; this never executes the product."""
import argparse
from collections import Counter
import copy
import datetime
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
WINDOWS_REVISION = '69e769be9baa4e825139b72ce1a9b48bf8143114'
PRODUCT_REVISION = '0fe03ba30da9600c25588d506d7e094df89cf56f'
PREVIOUS_DOCUMENTATION = '75c38f8'
LINUX_CANONICAL_REPORT = 'verification/package-v3-linux-notices-final.json'
LINUX_BINARY_SHA256 = 'e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42'
LINUX_TARGET = 'x86_64-unknown-linux-musl'
PACKAGING_REVISION = 'f5878bfa725db8c0dcab7e45ca3cf89159508975'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(name):
    return json.loads((ROOT/name).read_text(encoding='utf-8'))


def write(name, value):
    (ROOT/name).write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n', encoding='utf-8', newline='\n')


def archive(name):
    original = subprocess.check_output(['git', 'show', f'{PREVIOUS_DOCUMENTATION}:{name}'], cwd=ROOT)
    path = ROOT/name.replace('.json', '-before-language-installers-v3.json')
    if path.exists():
        if path.read_bytes() != original:
            raise ValueError('Historical documentation changed: '+str(path))
    else:
        with path.open('xb') as stream:
            stream.write(original)
    return path.relative_to(ROOT).as_posix(), json.loads(original)


def binding(name, note=None):
    value = load(name)
    result = {'report': name, 'report_sha256': sha(ROOT/name)}
    for key in ('scope', 'status', 'binary_sha256', 'source_head_sha', 'exit_code', 'counts', 'matrix',
                'source_review', 'documentation_evidence_review', 'release_approval'):
        if key in value:
            result[key] = value[key]
    if note:
        result['note'] = note
    receipt_name = name.removesuffix('.json')+'.command.json'
    if not name.endswith('.command.json') and (ROOT/receipt_name).is_file():
        receipt = load(receipt_name)
        if receipt.get('report_sha256') != result['report_sha256']:
            raise ValueError('Receipt digest differs: '+receipt_name)
        result.update(command_receipt=receipt_name, command_receipt_sha256=sha(ROOT/receipt_name),
                      actual_driver_exit_code=receipt['exit_code'])
    return result


def bound_file(reference):
    path = (ROOT/reference['path']).resolve(strict=True)
    path.relative_to(ROOT.resolve())
    if not path.is_file() or sha(path) != reference['sha256']:
        raise ValueError('Evidence digest differs: '+reference['path'])
    return path


def linux_evidence(name):
    """Validate the exact native candidate and classify each original criterion."""
    if name != LINUX_CANONICAL_REPORT:
        raise ValueError('Only the final canonical native Linux report is applicable')
    if not isinstance(LINUX_BINARY_SHA256, str) or len(LINUX_BINARY_SHA256) != 64:
        raise ValueError('The fixed Linux candidate has not been built and selected yet')
    value = load(name)
    metadata = value.get('build_metadata', {})
    if (value.get('scope') != 'ACTUAL_LINUX_VM_BUILD_WORKSPACE_GATES_SMOKES_AND_INSTALLERS'
            or value.get('app_build_revision') != PRODUCT_REVISION
            or value.get('packaging_revision') != PACKAGING_REVISION
            or value.get('bundled_docs_revision') != PACKAGING_REVISION
            or value.get('linux_target') != LINUX_TARGET
            or metadata.get('revision') != PRODUCT_REVISION
            or metadata.get('target') != LINUX_TARGET
            or metadata.get('profile') != 'release'
            or metadata.get('tracked_source_dirty') is not False
            or value.get('binary_sha256') != LINUX_BINARY_SHA256
            or value.get('elf', {}).get('format') != 'ELF64_X86_64'
            or value.get('elf', {}).get('pt_interp', 'missing') is not None):
        raise ValueError('Linux source, target, metadata or ELF differs from the selected candidate')
    if value.get('status') not in ('PASS', 'PARTIAL', 'FAIL'):
        raise ValueError('Pending or diagnostic reports are not executed Linux acceptance')
    source = value['source_manifest']
    bound_file(source)
    if source.get('checked_files', 0) <= 0:
        raise ValueError('Linux source manifest has no checked files')
    commands = value.get('commands', [])
    for command in commands:
        if not isinstance(command.get('exit_code'), int):
            raise ValueError('Every reported Linux command needs a real exit code')
        if command.get('status') != ('PASS' if command['exit_code'] == 0 else 'FAIL'):
            raise ValueError('Linux command status does not match its actual exit code')
        bound_file({'path': command['log'], 'sha256': command['log_sha256']})
    metadata_checks = [c for c in commands if c['exit_code'] == 0
                       and isinstance(c.get('argv'), list) and c['argv'][-1:] == ['metadata']]
    build_checks = [c for c in commands if c['exit_code'] == 0
                    and isinstance(c.get('argv'), list)
                    and any(Path(arg).name == 'cargo' for arg in c['argv'])
                    and all(arg in c['argv'] for arg in ('build', '--release', '--locked'))]
    artifacts = value.get('artifacts', [])
    for artifact in artifacts:
        path = bound_file(artifact)
        if path.stat().st_size != artifact['bytes']:
            raise ValueError('Linux artifact size differs: '+artifact['path'])
    native_elf = [a for a in artifacts if a['sha256'] == LINUX_BINARY_SHA256]
    ac1 = bool(metadata_checks and build_checks and native_elf)
    archive_kinds = {suffix for suffix in ('.deb', '.tar.gz')
                     if any(a['path'].endswith(suffix) for a in artifacts)}
    inventory = value.get('payload_inventory', {})
    packaging_source = value.get('packaging_source_manifest', {})
    lifecycle = value.get('installer_lifecycle', {})
    ac3 = False
    if archive_kinds == {'.deb', '.tar.gz'} and inventory and packaging_source:
        bound_file(inventory)
        bound_file(packaging_source)
        ac3 = (inventory.get('entries', 0) > 0 and packaging_source.get('checked_files', 0) > 0
               and value.get('product_equivalence', {}).get('status') == 'PASS'
               and value.get('product_equivalence', {}).get('compared_files', 0) > 0
               and lifecycle.get('status') == 'PASS'
               and all(lifecycle.get(key) == 0 for key in ('install_exit_code', 'reinstall_exit_code', 'uninstall_exit_code')))
        if ac3:
            bound_file(lifecycle['report'])
    signal = value.get('signal_shutdown_acceptance', {})
    if signal.get('requirement') != 'ODIN-029-A2' or signal.get('status') != 'NOT_RUN':
        raise ValueError('Dedicated signal/descendant/journal acceptance requires separate review; never infer it from cancellation')
    criteria = {
        'ODIN-029-A1': {'status': 'PARTIAL' if ac1 else 'NOT_RUN',
            'remaining': 'Source-bound Linux native build and actual metadata execution only; macOS and the complete platform matrix remain unverified.' if ac1 else 'No applicable successful source-bound native build/metadata/ELF set.'},
        'ODIN-029-A2': {'status': 'NOT_RUN',
            'remaining': 'Dedicated SIGTERM/SIGINT LSP/DAP descendant cleanup and journal acceptance was not run. General MCP cancellation is not this criterion.'},
        'ODIN-029-A3': {'status': 'PARTIAL' if ac3 else 'NOT_RUN',
            'remaining': 'Linux source/package manifests, payload inventory and archive hashes were checked; macOS artifacts remain unverified.' if ac3 else 'Complete source/package manifest, payload inventory and archive checksum evidence is not available.'},
    }
    result = binding(name)
    result['acceptance_results'] = criteria
    result['app_build_revision'] = value['app_build_revision']
    result['packaging_revision'] = value.get('packaging_revision')
    result['bundled_docs_revision'] = value.get('bundled_docs_revision')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--linux-report', help='Completed canonical Linux native report, if available')
    parser.add_argument('--review-report', help='Independent review report, if available')
    args = parser.parse_args()
    tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
    product_files = sorted(name for name in tracked if name.startswith(('crates/', 'schemas/', 'dashboard/')) or name in ('Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml'))
    for name in product_files:
        compiled = subprocess.check_output(['git', 'show', f'{PRODUCT_REVISION}:{name}'], cwd=ROOT)
        if (ROOT/name).read_bytes() != compiled:
            raise ValueError('Product differs from the selected Linux source revision: '+name)
    delivery_history, old_delivery = archive('verification/delivery-status.json')
    evidence_history, evidence = archive('verification/feature-evidence.json')
    now = datetime.datetime.now(datetime.UTC).isoformat()
    build = load('verification/language-installers-v3-windows-build-final.json')
    continuation = load('verification/language-installers-v3-continuation-01.json')
    reports = {
        'windows_build': binding('verification/language-installers-v3-windows-build-final.json', 'Compile exit 0; metadata execution blocked, no runtime acceptance.'),
        'windows_target_fmt_clippy': binding('verification/language-installers-v3-windows-static-final-01.json', 'Earlier69 Windows-target fmt and all-target strict Clippy exit 0; blocked release app hash unchanged and never executed.'),
        'windows_protocol_final_02': binding('verification/language-installers-v3-windows-protocol-final-02.json', 'Current combined source: fmt, protocol library tests and all-target strict Clippy exit 0; retained release candidate never executed or rebuilt.'),
        'windows_host_block': binding('verification/language-installers-v3-final-host-block.json'),
        'windows_workspace_attempt': binding('verification/language-installers-v3-workspace-all-targets-01.json', 'Older source: 214 passed / 5 failed / 6 ignored; five stdin workload starts blocked by host policy. Not replaced by independent continuation.'),
        'windows_independent_continuation': binding('verification/language-installers-v3-continuation-01.json', '13 independent commands exit 0 against the 780c6ad candidate; not the final 69e769b candidate.'),
        'isolated_pid_protocol': binding('verification/language-installers-v3-isolated-pid-protocol-01.json', '31 protocol fixture regressions passed after null-PID change; not real Csharp acceptance.'),
        'five_language_first_attempt': binding('verification/language-sandbox-v3-five-native-01.json', 'Preserve actual failing first attempt: 150 PASS / 4 FAIL / 4 UNAVAILABLE / 12 NOT_VERIFIED.'),
        'go_native': binding('verification/language-sandbox-v3-go-native-02.json'),
        'python_native': binding('verification/language-sandbox-v3-python-native-02.json'),
        'ruff_native': binding('verification/language-sandbox-v3-ruff-native-01.json'),
        'csharp_direct': binding('verification/language-sandbox-v3-direct-csharp-nullpid-07.command.json', 'Real semantic symbols and server exit 0; diagnostic driver cleanup exit 1. No full MCP or final-binary PASS.'),
        'windows_installer_780': binding('verification/package-v3-windows-candidate.json', '22 install/reinstall/uninstall checks passed for older 780c6ad; final 69 Windows packaging remains blocked.'),
        'linux_retired_69_failure': binding('verification/package-v3-linux-69-diagnostic.json', 'Actual older Linux build/metadata passed, but workspace39PASS/6FAIL and MCP15PASS/1FAIL exposed Unix fsync. No installer was released for that broken candidate.'),
        'linux_retired_d3_failure': binding('verification/package-v3-linux-d3-diagnostic.json', 'Core39PASS after Unix fix; full suite192PASS/19FAIL/3ignored exposed fixture Python dependency and two protocol assertions. Release was stopped with -15, no ELF/installer acceptance inferred.'),
        'linux_initial_functional_package': binding('verification/package-v3-linux-final.json', 'Original e64/0fe application gates and a839 functional package acceptance; retained unchanged before completing toolchain/native runtime notices. The new documentation-only package reuses the same executed application evidence.'),
        'evidence_integrity': binding('verification/language-sandbox-v3-evidence-index-01.json', 'Offline 18-receipt/source/transcript integrity only, no backend execution.'),
        'prepared_final_matrix': binding('verification/language-sandbox-v3-finalrun-plan.json', 'Plan only; 0 final-candidate LSP projects executed.'),
        'qa_report_preflight': binding('verification/language-installers-v3-continuation-preflight-01.json', 'Five inert overwrite-protection checks; zero native process calls.'),
        'qa_failed_runner_rejection': binding('verification/package-v3-evidence-outcomes-01.json', 'Six inert failure-classification checks and independently reviewed exact helper hashes; no product or backend process calls.'),
        'qa_linux_criteria_classification': binding('verification/language-installers-v3-linux-classification-03.json', 'Twelve inert criterion-classification checks against the final assembler source; not native Linux execution.'),
        'runtime_notice_sources': binding('verification/runtime-notices-v3-sources.json', 'Exact upstream archive/source and runtime notice inventory, not a legal compliance certification.'),
        'runtime_notice_checks': binding('verification/runtime-notices-v3-checks.json', '94 executed offline archive, notice segment and source-fidelity checks; no application or backend execution.'),
        'qa_notice_repack_guards': binding('verification/package-v3-notices-evidence-outcomes-01.json', 'Six inert evidence guards for a documentation-only repack; original application execution is explicitly retained rather than rerun.'),
    }
    linux = linux_evidence(args.linux_report) if args.linux_report else {
        'status': 'IN_PROGRESS', 'acceptance_status': 'NOT_RUN', 'selected_source_head_sha': PRODUCT_REVISION,
        'scope': 'Fixed-source candidate in disposable Debian 12 QEMU guest. Prior69 native suite failed Unix fsync; replacement gates/build/packaging are not inferred PASS.'}
    review = binding(args.review_report) if args.review_report else {'status': 'IN_PROGRESS', 'release_approval': 'BLOCKED'}
    current = {
        'scope': 'FIVE_ADDITIONAL_NATIVE_SANDBOX_LANGUAGES_AND_WINDOWS_LINUX_INSTALLERS',
        'status': 'PARTIAL', 'current_product_source_head_sha': PRODUCT_REVISION,
        'product_source_hashes_match_selected_linux_revision': True,
        'current_product_sha256': {name: sha(ROOT/name) for name in product_files},
        'windows_final_candidate': {'status': 'BLOCKED', 'binary_path': 'E:/odin/target/release/odin.exe',
            'compiled_source_head_sha': WINDOWS_REVISION,
            'source_relation': 'Retained Windows69 candidate. Current source additionally fixes Unix-only directory sync and rejects raw UNC URI prefixes before normalization; no current-source Windows rebuild or runtime acceptance is inferred.',
            'binary_sha256': build['binary_sha256'], 'build_exit_code': build['exit_code'],
            'metadata_executed': False, 'metadata_exit_code': None,
            'administrator_policy_allowance_confirmed': False,
            'six_language_matrix': 'NOT_RUN', 'final_installer': 'NOT_RUN'},
        'historical_working_windows_candidate': {'source_revision': continuation['binary_metadata']['revision'],
            'binary_sha256': continuation['binary_sha256'],
            'native_languages': {'rust': 'PASS_TWO_PROJECTS', 'go': 'PASS_TWO_PROJECTS',
                'python': 'PASS_TWO_PROJECTS_WITH_EXPLICIT_PRIVATE_QA_BACKEND_ASSETS',
                'typescript': 'PARTIAL_UNVERSIONED_DIAGNOSTICS', 'java': 'PARTIAL_UNVERSIONED_DIAGNOSTICS',
                'csharp': 'FAIL_MCP_DIRECT_NULL_PID_DIAGNOSTIC_ONLY'},
            'all_six_final_candidate_accepted': False},
        'linux': linux, 'independent_review': review, 'reports': reports,
        'all_40_requirements_accepted': False, 'all_serena_features_implemented': False,
        'cloud_ci_started': False,
    }
    delivery = {
        'project': 'Odin', 'version': '0.2.0', 'release_class': 'NATIVE_PREVIEW_WITH_WINDOWS_HOST_BLOCK',
        'generated_at_utc': now, 'architecture': old_delivery['architecture'],
        'workspace_crates': 4, 'tool_contracts': 85, 'serena_comparison_entries': 53,
        'serena_dependency': False, 'proven_faster_than_serena': False,
        'all_40_requirements_accepted': False, 'all_serena_features_implemented': False,
        'cloud_ci_started': False, 'current_extension': current,
        'evidence': reports, 'historical_delivery': binding(delivery_history, 'Prior snapshot preserved verbatim; its PASS statuses apply only to its own source/binary.'),
        'publication': {'status': 'NOT_RUN_FOR_THIS_EXTENSION', 'url': 'https://github.com/stevenke1981/odin',
            'prior_publication': old_delivery['publication'], 'binary_release_created': False},
        'language_profiles': old_delivery['language_profiles'],
        'security_boundaries': [
            'Explicit host grants only; formatters require write plus formatter permission and native read-only-source sandbox. All source edits use expected-hash Transactions.',
            'Enumeration reveals directory names only with explicit host configuration; no implicit readable coverage or source write grants.',
            'Python QA uses per-project pinned private writable backend copies to handle native SetCurrentDirectory behavior. Never execute these mutable copies unsandboxed, promote them to trusted installed tools, or redistribute them.',
            'Csharp uses explicit OmniSharp selection; sandbox LSP initializes processId=null because the native lifetime owns cleanup. No host-PID monitoring across isolation.',
            'Linux native execution/packaging does not establish Linux OS sandbox support. Inno Setup packages Windows; Linux uses deb and portable tar.'
        ],
        'incomplete_or_unverified': [
            'Final Windows metadata, six-language/twelve-project MCP matrix and final installer await administrator policy allowance for the unchanged candidate.',
            'Windows workspace suite retains five actual host-policy failures; earlier independent PASS commands do not replace that failed gate.',
            'TypeScript/Java unversioned diagnostics remain NOT_VERIFIED; unadvertised capabilities remain UNAVAILABLE. Csharp direct symbols are not full MCP acceptance.',
            'Linux/macOS sandbox support, macOS native builds, complete resource/device isolation, and OS-sandbox DAP debuggee matrix remain unverified or unimplemented.',
            'All-language IDE parity, every rule/refactor, clean Windows machine, cross-version/interrupted installation, signatures, benchmarks and formal security audit remain incomplete.'
        ]}
    evidence.update(release_class=delivery['release_class'], observed_date=now[:10],
        source_head_sha_at_documentation_review=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        scope='Current v3 extension below; prior source-bound checks remain historical and do not certify the blocked final Windows candidate.',
        previous_documentation=binding(evidence_history), language_installers_v3=current)
    evidence['final_integrated_candidate'] = copy.deepcopy(current['windows_final_candidate'])
    evidence['final_integrated_candidate']['compiled_source_head_sha'] = WINDOWS_REVISION
    evidence['final_integrated_candidate_scope'] = 'Current compiled Windows candidate; execution remains blocked. Complete prior candidate objects are preserved in previous_documentation.'
    evidence['publication'] = copy.deepcopy(delivery['publication'])
    evidence['final_artifacts'] = {'windows_current': 'BLOCKED', 'windows_historical_780': reports['windows_installer_780'], 'linux': linux}
    evidence['final_windows_packaging'] = {'status': 'BLOCKED', 'final_candidate_packaged': False,
        'historical_780_candidate': reports['windows_installer_780'], 'blocking_evidence': reports['windows_host_block']}
    evidence['real_backend_validation'] = {'status': 'BLOCKED_FOR_FINAL_WINDOWS_CANDIDATE',
        'historical_780_continuation': reports['windows_independent_continuation'],
        'final_six_language_plan': reports['prepared_final_matrix']}
    evidence['extension_evidence']['evidence_scope'] = 'Historical earlier 85-tool extension; current v3 candidate is language_installers_v3.'
    summaries = {
        'ODIN-001': '69e769b Windows release build exit0；metadata被AppControl4551阻擋。歷史780 suite214PASS/5FAIL/6ignored，13獨立命令PASS不能替代失敗gate。Linux native狀態見language_installers_v3.linux；macOS未驗收。',
        'ODIN-007': '新增沙箱LSP processId=null；31 protocol fixtures PASS。780上的Rust/Go/Python已實跑兩專案，TS/Java保留未版本診斷，C#全MCP及final69六語矩陣受主機控管阻擋。',
        'ODIN-008': '780 native真實語意查詢已取得Rust/Go/Python及TS/Java限定證據；C#nullPID direct取得真symbols但driver cleanup exit1，不能視為最終MCP PASS。',
        'ODIN-009': '新增明確host Ruff stdin formatter與CAS preview/apply/exact rollback；780 native formatter18 checks PASS、Python雙專案60 PASS/4 UNAVAILABLE。final69尚不能啟動。',
        'ODIN-010': '真實後端驗收分來源快照與binary hash；780有Go46/Python60及Rust/TS/Java證據，C#與final69全矩陣仍不完整，未用mock替代。',
        'ODIN-012': '新增明確host enumerate-only grants與唯讀來源多語profile；780五語首輪150PASS/4FAIL/12NOT_VERIFIED/4UNAVAILABLE保留，Go/Python後續獨立實測PASS。final69全矩陣BLOCKED；Linux native包不代表Linux sandbox。',
        'ODIN-017': '780真Clippy/Ruff/ESLint30 checks與native Ruff formatter18 checks有實跑證據；TS/Java未版本診斷仍NOT_VERIFIED，全規則去重矩陣未完成。',
        'ODIN-019': '28 registry entries是宣告指引。新增五語native workload保留逐binary證據；Go/Python通過限定雙專案，C#全MCP和final69六語仍待驗收。',
        'ODIN-027': '手動self-hosted CI政策未變，未啟動cloud CI。Windows workspace exit101及AppControl4551明確保留，13獨立PASS不取代全suite；Linux native結果另綁來源。',
        'ODIN-028': '780c6ad portable/Inno7.1實際install/reinstall/uninstall22 checks PASS。69e769b已編譯但4551阻擋，最終Windows安裝包未實跑；未驗跨版/乾淨機/簽章。',
        'ODIN-029': 'Linux69發現Unix fsync失敗，d3核心39PASS後全suite另揭露fixture依賴及兩項protocol測試；整合修正0fe03ba的native build/gates/deb與tar以language_installers_v3.linux實際報告為準，不轉移舊結果。macOS仍NOT_RUN，Linux sandbox未實作。',
        'ODIN-035': '780真native analyzer30 checks與Ruff stdin formatter18 checks PASS，新增輸出完整性/權限/expected-hash拒絕路徑；final69與所有IDE規則等價未驗收。',
        'ODIN-040': '本輪來源/命令/binary/逐語言transcript與18 receipt離線索引已綁定；保留原失敗、初次觀察更正與blocked final69，不讓歷史PASS流入新版。',
    }
    relevant = set(summaries)
    for entry in evidence['requirements']:
        entry['checks_scope'] = 'Historical checks retain their original source/binary scope. Current v3 evidence is language_installers_v3; final Windows runtime is blocked.'
        if entry['id'] in relevant:
            entry['historical_summary'] = entry['summary']
            entry['summary'] = summaries[entry['id']]
            entry['language_installers_v3'] = {'status': 'PARTIAL', 'ref': '#/language_installers_v3'}
    # A documentation assembler never upgrades the original acceptance criteria to PASS.
    if args.linux_report:
        entry = next(x for x in evidence['requirements'] if x['id'] == 'ODIN-029')
        applicable = any(item['status'] == 'PARTIAL' for item in linux['acceptance_results'].values())
        entry['status'] = 'PARTIAL' if applicable else 'NOT_RUN'
        entry['implementation'] = 'NATIVE_SOURCE_PARTIAL' if applicable else 'NOT_VERIFIED'
        entry['language_installers_v3']['status'] = entry['status']
        entry['supporting_reports'].append(copy.deepcopy(linux))
        for item in entry['acceptance_results']:
            item.update(linux['acceptance_results'][item['id']])
        for item in entry['unverified_acceptance']:
            item['remaining'] = linux['acceptance_results'][item['id']]['remaining']
    evidence['status_summary'] = dict(Counter(entry['status'] for entry in evidence['requirements']))
    features = load('requirements/features.json')
    for entry in features['features']:
        actual = next(item for item in evidence['requirements'] if item['id'] == entry['id'])
        entry['evidence'] = actual['status']+'; verification/feature-evidence.json#'+entry['id']
        entry['current'] = actual['implementation']
        if entry['id'] in relevant:
            entry['implementation_summary'] = actual['summary']
    features['note'] = 'Current v3 scope is verification/feature-evidence.json language_installers_v3; historical results do not certify the blocked final Windows candidate. No requirement is fully accepted.'
    write('verification/delivery-status.json', delivery)
    write('verification/feature-evidence.json', evidence)
    write('requirements/features.json', features)
    print(json.dumps({'scope': 'DOCUMENTATION_ASSEMBLY_ONLY', 'product_executions': 0,
        'product_revision': PRODUCT_REVISION, 'status_summary': evidence['status_summary'],
        'windows_final_candidate': 'BLOCKED', 'linux': linux.get('status')}))


if __name__ == '__main__':
    main()
