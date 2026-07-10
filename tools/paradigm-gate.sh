#!/usr/bin/env bash
# EvoRule Paradigm Gate Runner v2.0
# Source: GATES.md (canonical) + TCB primitive mechanism and boundary handling specification §5.1 / §10.1
# Usage: ./tools/paradigm-gate.sh [--quick|--full|--tcb]
# Exit codes: 0 = all pass, 1 = gate failure, 2 = internal error

set -u

# Resolve repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
while [ "$REPO_ROOT" != "/" ]; do
    if [ -d "$REPO_ROOT/.git" ] || [ -f "$REPO_ROOT/Cargo.toml" ]; then
        break
    fi
    REPO_ROOT="$(dirname "$REPO_ROOT")"
done
if [ "$REPO_ROOT" = "/" ]; then
    echo "ERROR: Could not find repo root (no .git/ or Cargo.toml found)" >&2
    exit 2
fi
cd "$REPO_ROOT"

# ---------- Colors ----------
if [ -t 1 ]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'
    BLUE='\033[0;34m'; BOLD='\033[1m'; RESET='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BLUE=''; BOLD=''; RESET=''
fi

# ---------- Counters ----------
PASS=0; FAIL=0; SKIP=0
declare -a FAILED_GATES=()

log_pass() { echo -e "${GREEN}[PASS]${RESET} $1"; PASS=$((PASS+1)); }
log_fail() { echo -e "${RED}[FAIL]${RESET} $1"; FAIL=$((FAIL+1)); FAILED_GATES+=("$1"); }
log_skip() { echo -e "${YELLOW}[SKIP]${RESET} $1"; SKIP=$((SKIP+1)); }
log_info() { echo -e "${BLUE}[INFO]${RESET} $1"; }
section() { echo; echo -e "${BOLD}=== $1 ===${RESET}"; }

# ---------- File lists ----------
get_rs_files() {
    # Exclude `benches/` directories: they hold developer-facing benchmark
    # harnesses (cargo bench), which are dev-only and never compiled into
    # the release artifact. Wall-clock timing is permitted there by design.
    # All other .rs files (src/, tests/, examples/) remain in scope.
    git ls-files 2>/dev/null | grep -E '\.rs$' | grep -vE '/benches/' || true
}
get_json_files() {
    git ls-files 2>/dev/null | grep -E '\.json$' || true
}
get_tcb_rs_files() {
    git ls-files 2>/dev/null | grep -E '^crates/tcb/src/.*\.rs$' || true
}
get_handler_rs_files() {
    git ls-files 2>/dev/null | grep -E 'crates/.*/handlers/.*\.rs$' || true
}
get_all_tracked() {
    git ls-files 2>/dev/null || true
}

# ---------- Helper: scan regex ----------
scan_pattern() {
    local pattern="$1"
    local files_cmd="$2"
    local hits
    hits=$($files_cmd | xargs -r grep -nE "$pattern" 2>/dev/null || true)
    if [ -n "$hits" ]; then
        echo "$hits"
        return 0
    fi
    return 1
}

# ---------- Helper: detect Chinese in code only ----------
# Excludes doc comments (/// and //!) and markdown files
detect_chinese_in_code() {
    local files_cmd="$1"
    local temp_file
    temp_file=$(mktemp)
    $files_cmd > "$temp_file" 2>/dev/null || return 0

    local hits=""
    while IFS= read -r file; do
        [ -z "$file" ] && continue
        # Skip markdown files
        if echo "$file" | grep -qE '\.md$'; then
            continue
        fi
        # For .rs and .json, check code lines only
        if echo "$file" | grep -qE '\.rs$|\.json$'; then
            # Remove doc comments (///, //!), then check for Chinese
            local file_hits
            file_hits=$(
                grep -vE '^[[:space:]]*///|^[[:space:]]*//!' "$file" 2>/dev/null \
                | grep -nP '[\x{4E00}-\x{9FFF}]' 2>/dev/null || true
            )
            if [ -n "$file_hits" ]; then
                hits="${hits}${file}:${file_hits}"$'\n'
            fi
        fi
    done < "$temp_file"
    rm -f "$temp_file"

    if [ -n "$hits" ]; then
        echo "$hits"
        return 0
    fi
    return 1
}

# ============================================================
# A. Mechanical Gates
# ============================================================

gate_g01_no_unsafe_in_tcb() {
    local tcb_rs
    tcb_rs=$(get_tcb_rs_files)
    [ -z "$tcb_rs" ] && { log_skip "G-01 no unsafe in TCB (no TCB .rs files)"; return 0; }
    local hits
    hits=$(echo "$tcb_rs" | xargs -r grep -nE '\bunsafe\b' 2>/dev/null \
        | grep -vE ':[[:space:]]*//' || true)
    if [ -n "$hits" ]; then
        log_fail "G-01 no unsafe in TCB (ER-603 / §2.2)"
        echo "$hits" | sed 's/^/    /'
        return 1
    fi
    log_pass "G-01 no unsafe in TCB"
    return 0
}

gate_g02_no_wallclock() {
    local rs_files
    rs_files=$(get_rs_files)
    [ -z "$rs_files" ] && { log_skip "G-02 no wall-clock time"; return 0; }
    local hits
    hits=$(echo "$rs_files" | xargs -r grep -nE 'SystemTime::now\(\)|Instant::now\(\)' 2>/dev/null || true)
    if [ -n "$hits" ]; then
        log_fail "G-02 no wall-clock time (§2.2 / §7.2)"
        echo "$hits" | sed 's/^/    /'
        echo "    Use LogicalClock::current_tick() instead"
        return 1
    fi
    log_pass "G-02 no wall-clock time"
    return 0
}

gate_g03_no_uuid_v4() {
    local rs_files
    rs_files=$(get_rs_files)
    [ -z "$rs_files" ] && { log_skip "G-03 no UUID v4"; return 0; }
    local hits
    hits=$(echo "$rs_files" | xargs -r grep -nE 'Uuid::new_v4\(\)' 2>/dev/null || true)
    if [ -n "$hits" ]; then
        log_fail "G-03 no UUID v4 (§2.2 / §7.2)"
        echo "$hits" | sed 's/^/    /'
        echo "    Use content_hash(&content) instead"
        return 1
    fi
    log_pass "G-03 no UUID v4"
    return 0
}

gate_g04_no_rng() {
    local rs_files
    rs_files=$(get_rs_files)
    [ -z "$rs_files" ] && { log_skip "G-04 no RNG"; return 0; }
    local hits
    hits=$(echo "$rs_files" | xargs -r grep -nE 'rand::random\(\)|thread_rng\(\)' 2>/dev/null || true)
    if [ -n "$hits" ]; then
        log_fail "G-04 no RNG (§2.2 / §7.2)"
        echo "$hits" | sed 's/^/    /'
        echo "    Use DeterministicRNG::from_seed(seed) instead"
        return 1
    fi
    log_pass "G-04 no RNG"
    return 0
}

gate_g05_no_env_var() {
    local rs_files
    rs_files=$(get_rs_files)
    [ -z "$rs_files" ] && { log_skip "G-05 no direct env::var"; return 0; }
    # More precise: match env::var( but exclude those used with State.__env__
    local hits
    hits=$(echo "$rs_files" \
        | xargs -r grep -nE 'env::var\(' 2>/dev/null \
        | grep -vE 'State.*__env__|__env__.*State' || true)
    if [ -n "$hits" ]; then
        log_fail "G-05 no direct env::var (§7.2 — use startup snapshot to State.__env__)"
        echo "$hits" | sed 's/^/    /'
        return 1
    fi
    log_pass "G-05 no direct env::var"
    return 0
}

gate_g06_no_deprecated_types() {
    local rs_files
    rs_files=$(get_rs_files)
    [ -z "$rs_files" ] && { log_skip "G-06 no deprecated types"; return 0; }

    # 17 deprecated types from §3.2 / §6.2
    local deprecated_patterns=(
        "DimensionChecker::new"
        "DimensionChecker::check"
        "BackwardChainer::new"
        "BackwardChainer::chain"
        "ConvergenceChecker::new"
        "ConvergenceChecker::check"
        "InformationGainCalculator::new"
        "InformationGainCalculator::calculate"
        "Planner::new"
        "Planner::plan"
        "EffectPredictor::new"
        "EffectPredictor::predict"
        "CycleDetector::new"
        "CycleDetector::detect"
        "ConflictDetector::new"
        "ConflictDetector::detect"
        "SolverValidator::new"
        "SolverValidator::validate"
        "SelfCheckConfig::new"
        "SelfCheck::run"
        "RedlineChecker::new"
        "RedlineChecker::check"
        "ConstitutionalGate::new"
        "ConstitutionalGate::evaluate"
        "Pipeline::new"
        "PipelineBuilder::new"
        "ConfigDrivenStrategy::new"
        "MetaExecutor::new"
        "MetaExecutor::execute"
        "AdaptiveMeta::new"
        "AdaptiveMeta::cycle"
        "InjectPruneExecutor::new"
        "InjectPruneExecutor::execute"
    )

    local total_hits=""
    for pat in "${deprecated_patterns[@]}"; do
        local hits
        hits=$(echo "$rs_files" | xargs -r grep -nF "$pat" 2>/dev/null || true)
        if [ -n "$hits" ]; then
            total_hits="${total_hits}${hits}"$'\n'
        fi
    done

    if [ -n "$total_hits" ]; then
        log_fail "G-06 no deprecated types (§6.2 / V-01)"
        echo "$total_hits" | sed 's/^/    /'
        echo "    Use ForwardChain::infer() + JSON rules instead"
        return 1
    fi
    log_pass "G-06 no deprecated types"
    return 0
}

# G-06a: Handler must NOT call business logic (specialized error message)
gate_g06a_handler_no_business_call() {
    local handler_files
    handler_files=$(get_handler_rs_files)
    [ -z "$handler_files" ] && { log_skip "G-06a handler no business calls (no handler files)"; return 0; }

    local business_patterns=(
        "DimensionChecker"
        "BackwardChainer"
        "ConvergenceChecker"
        "InformationGainCalculator"
        "Planner"
        "EffectPredictor"
        "CycleDetector"
        "ConflictDetector"
        "SolverValidator"
        "SelfCheckConfig"
        "SelfCheck::run"
        "RedlineChecker"
        "ConstitutionalGate"
        "Pipeline::new"
        "PipelineBuilder::new"
        "ConfigDrivenStrategy"
        "MetaExecutor"
        "AdaptiveMeta"
        "InjectPruneExecutor"
    )

    local total_hits=""
    for pat in "${business_patterns[@]}"; do
        local hits
        hits=$(echo "$handler_files" | xargs -r grep -nE "${pat}::(new|check|chain|plan|detect|calculate|predict|validate|run|evaluate|execute|cycle)" 2>/dev/null || true)
        if [ -n "$hits" ]; then
            total_hits="${total_hits}${hits}"$'\n'
        fi
    done

    if [ -n "$total_hits" ]; then
        log_fail "G-06a handler must NOT call business logic (§6.2 / V-02)"
        echo "$total_hits" | sed 's/^/    /'
        echo "    Handlers are adapter layer: parse input → trigger rule engine → serialize output"
        echo "    Move business logic to JSON rules, not Rust handlers"
        return 1
    fi
    log_pass "G-06a handler no business calls"
    return 0
}

gate_g07_no_func_eval_in_json() {
    local json_files
    json_files=$(get_json_files)
    [ -z "$json_files" ] && { log_skip "G-07 no \$func/\$eval in JSON"; return 0; }
    local hits
    hits=$(echo "$json_files" | xargs -r grep -nE '"\$func"|"\$eval"' 2>/dev/null || true)
    if [ -n "$hits" ]; then
        log_fail "G-07 no \$func/\$eval in JSON (ER-606 / §8.6)"
        echo "$hits" | sed 's/^/    /'
        echo "    Use concrete values or \$ref instead"
        return 1
    fi
    log_pass "G-07 no \$func/\$eval in JSON"
    return 0
}

gate_g08_no_forbidden_transform() {
    local json_files
    json_files=$(get_json_files)
    [ -z "$json_files" ] && { log_skip "G-08 no forbidden transform"; return 0; }
    local hits
    hits=$(echo "$json_files" | xargs -r grep -nE '"type":\s*"(if_else|for_each|iterate_list|lambda|call)"' 2>/dev/null || true)
    if [ -n "$hits" ]; then
        log_fail "G-08 no forbidden transform types (§4.4)"
        echo "$hits" | sed 's/^/    /'
        echo "    Use evaluate_domain / while_loop / \$ref instead"
        return 1
    fi
    log_pass "G-08 no forbidden transform types"
    return 0
}

gate_g09_no_chinese_in_code() {
    local all_files
    all_files=$(get_all_tracked)
    [ -z "$all_files" ] && { log_skip "G-09 no Chinese in code (no files)"; return 0; }

    # Detect Chinese in .rs and .json files, excluding doc comments
    local temp_list
    temp_list=$(mktemp)
    echo "$all_files" | grep -E '\.rs$|\.json$' > "$temp_list" 2>/dev/null || { rm -f "$temp_list"; log_skip "G-09 no Chinese in code (no .rs/.json files)"; return 0; }

    local hits=""
    while IFS= read -r file; do
        [ -z "$file" ] && continue
        # Extract lines with Chinese characters, excluding doc comments
        local file_hits
        file_hits=$(
            grep -vE '^[[:space:]]*///|^[[:space:]]*//!' "$file" 2>/dev/null \
            | grep -nP '[\x{4E00}-\x{9FFF}]' 2>/dev/null || true
        )
        if [ -n "$file_hits" ]; then
            hits="${hits}${file}:${file_hits}"$'\n'
        fi
    done < "$temp_list"
    rm -f "$temp_list"

    if [ -n "$hits" ]; then
        log_fail "G-09 no Chinese in code (release blocker)"
        echo "$hits" | sed 's/^/    /'
        echo "    Chinese characters are only permitted in /// and //! doc comments"
        echo "    Code logic, variable names, and JSON keys must use English"
        return 1
    fi
    log_pass "G-09 no Chinese in code"
    return 0
}

# ============================================================
# B. TCB-Specific Redline Gates (from TCB primitive specification §5.1 / §10.1)
# ============================================================

gate_r07_state_immutable() {
    local tcb_rs
    tcb_rs=$(get_tcb_rs_files)
    [ -z "$tcb_rs" ] && { log_skip "R-07 State immutable (no TCB .rs files)"; return 0; }

    # Check for &mut State in function signatures within primitive/ and control/
    local hits
    hits=$(echo "$tcb_rs" | grep -E 'primitive/|control/' | xargs -r grep -nE 'fn.*&mut State' 2>/dev/null || true)
    if [ -n "$hits" ]; then
        log_fail "R-07 State immutable (TCB primitive specification §5.1)"
        echo "$hits" | sed 's/^/    /'
        echo "    TCB primitives must take &State (immutable) and return new State"
        echo "    Use state.set_path() to return a new State, never modify in-place"
        return 1
    fi
    log_pass "R-07 State immutable"
    return 0
}

gate_r08_no_exec_calls() {
    local tcb_rs hits
    tcb_rs=$(get_tcb_rs_files)
    [ -z "$tcb_rs" ] && { log_skip "R-08 no exec_ calls (no TCB .rs files)"; return 0; }

    # Check for exec_ function calls within primitive/ and control/.
    # Production intent: TCB primitives must not call other exec_ fns directly;
    # only the InstructionRegistry should dispatch.
    # False-positive exclusions (segments 55+58 audit, 2026-07-02):
    #   - registry dispatch calls (reg.execute / registry.execute)
    #   - exec_while_loop (white-listed control primitive)
    #   - `fn exec_xxx(...)` function definitions
    #   - inline comments (:[[:space:]]*//) and module-level docs (//! and ///)
    #   - test code (inside #[cfg(test)] mod tests { ... } blocks)
    hits=$(
        echo "$tcb_rs" \
        | grep -E 'primitive/|control/' \
        | xargs python3 -c '
import sys, re
TEST_START = re.compile(r"^\s*(?:#\[cfg\(test\)\]\s*$|mod\s+tests\s*\{)")
EXEC_CALL = re.compile(r"\bexec_[a-z_]+\s*\(")
EXCLUDE = re.compile(r"(reg\.execute|registry\.execute|exec_while_loop|\bfn\s+exec_|:\s*//)")
DOC_COMMENT = re.compile(r"^\s*//!|^\s*///")
for f in sys.argv[1:]:
    try:
        with open(f, "r", encoding="utf-8", errors="replace") as fh:
            lines = fh.readlines()
    except Exception:
        continue
    in_test = False
    depth = 0
    for ln_n, line in enumerate(lines, 1):
        s = line.rstrip("\n")
        if in_test:
            depth += s.count("{") - s.count("}")
            if depth <= 0:
                in_test = False
            continue
        if DOC_COMMENT.match(s):
            continue
        if TEST_START.match(s):
            in_test = True
            depth = s.count("{") - s.count("}")
            continue
        if EXEC_CALL.search(s) and not EXCLUDE.search(s):
            print(f"{f}:{ln_n}: {s.strip()}")
' 2>/dev/null
    )
    if [ -n "$hits" ]; then
        log_fail "R-08 no exec_ calls in TCB (programming spec sec 1.3)"
        echo "$hits" | sed 's/^/    /'
        echo "    TCB primitives must not call other exec_ functions directly"
        echo "    Only the InstructionRegistry should dispatch to exec_ functions"
        return 1
    fi
    log_pass "R-08 no exec_ calls in TCB"
    return 0
}

# ============================================================
# C. Schema Gates
# ============================================================

gate_g15_rule_id_format() {
    local json_files
    json_files=$(git ls-files 2>/dev/null | grep -E 'rules/.*\.json$' || true)
    [ -z "$json_files" ] && { log_skip "G-15 rule_id format (no rules/*.json)"; return 0; }

    local bad_hits=""
    for f in $json_files; do
        local rule_id=""
        # Try jq first
        if command -v jq >/dev/null 2>&1; then
            rule_id=$(jq -r '.rule_id // ""' "$f" 2>/dev/null || true)
        elif command -v python >/dev/null 2>&1; then
            rule_id=$(python -c "
import json, sys
try:
    with open('$f', encoding='utf-8') as fp:
        d = json.load(fp)
    print(d.get('rule_id', ''))
except Exception:
    sys.exit(1)
" 2>/dev/null || true)
        else
            log_skip "G-15 rule_id format (jq or python required for JSON parsing)"
            return 0
        fi
        if [ -z "$rule_id" ]; then
            continue
        fi
        if ! echo "$rule_id" | grep -qE '^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$'; then
            bad_hits="${bad_hits}$f: rule_id='$rule_id' does not match {namespace}.{category}.{name}"$'\n'
        fi
    done

    if [ -n "$bad_hits" ]; then
        log_fail "G-15 rule_id format (§4.6)"
        echo "$bad_hits" | sed 's/^/    /'
        return 1
    fi
    log_pass "G-15 rule_id format"
    return 0
}

gate_g16_no_skeleton_transform() {
    local json_files
    json_files=$(git ls-files 2>/dev/null | grep -E 'rules/.*\.json$' || true)
    [ -z "$json_files" ] && { log_skip "G-16 no skeleton transform (no rules/*.json)"; return 0; }

    local bad_hits=""
    for f in $json_files; do
        local result
        if command -v jq >/dev/null 2>&1; then
            # Use jq to check transform completeness
            result=$(jq -r '
                def is_skeleton:
                    .transform == null or .transform == {} or
                    (.transform.type == "instruction_sequence" and (.transform.params.instructions | length) < 2);
                if is_skeleton then "SKELETON" else "" end
            ' "$f" 2>/dev/null || true)
        elif command -v python >/dev/null 2>&1; then
            result=$(python -c "
import json, sys
try:
    with open('$f', encoding='utf-8') as fp:
        d = json.load(fp)
    t = d.get('transform', {})
    if not t or t == {}:
        print('SKELETON')
        sys.exit(0)
    if t.get('type') == 'instruction_sequence':
        insts = t.get('params', {}).get('instructions', [])
        if len(insts) < 2:
            print('SKELETON')
            sys.exit(0)
except Exception:
    sys.exit(1)
" 2>/dev/null || true)
        else
            log_skip "G-16 no skeleton transform (jq or python required)"
            return 0
        fi
        if [ "$result" = "SKELETON" ]; then
            bad_hits="${bad_hits}$f: skeleton transform (V-04: need >=2 instructions or non-empty transform)"$'\n'
        fi
    done

    if [ -n "$bad_hits" ]; then
        log_fail "G-16 no skeleton transform (V-04)"
        echo "$bad_hits" | sed 's/^/    /'
        echo "    Each rule should have at least 2 instructions in instruction_sequence"
        return 1
    fi
    log_pass "G-16 no skeleton transform"
    return 0
}

# ============================================================
# Main
# ============================================================
main() {
    local mode="${1:-all}"

    echo -e "${BOLD}EvoRule Paradigm Gate Runner v2.0${RESET}"
    echo -e "Repo: $REPO_ROOT"
    echo -e "Mode: $mode"
    echo

    section "A. Mechanical Gates (pre-commit, fast)"
    gate_g01_no_unsafe_in_tcb
    gate_g02_no_wallclock
    gate_g03_no_uuid_v4
    gate_g04_no_rng
    gate_g05_no_env_var
    gate_g06_no_deprecated_types
    gate_g06a_handler_no_business_call
    gate_g07_no_func_eval_in_json
    gate_g08_no_forbidden_transform
    gate_g09_no_chinese_in_code

    section "B. TCB-Specific Redline Gates"
    gate_r07_state_immutable
    gate_r08_no_exec_calls

    section "C. Schema Gates (JSON structure)"
    gate_g15_rule_id_format
    gate_g16_no_skeleton_transform

    section "Summary"
    echo -e "  ${GREEN}PASS${RESET}: $PASS"
    echo -e "  ${RED}FAIL${RESET}: $FAIL"
    echo -e "  ${YELLOW}SKIP${RESET}: $SKIP"
    if [ $FAIL -gt 0 ]; then
        echo
        echo -e "${RED}=== BLOCKED ===${RESET}"
        echo "Failed gates:"
        printf '  - %s\n' "${FAILED_GATES[@]}"
        echo
        echo "Fix the violations above, then re-run."
        echo "There is no --no-verify bypass. See GATES.md for the full gate catalog."
        exit 1
    fi
    echo
    echo -e "${GREEN}=== ALL GATES PASS ===${RESET}"
    echo
    echo "Note: B (compile-time) and D (process) gates run in CI, not here."
    echo "See .github/workflows/paradigm-gate.yml for the full pipeline."
    exit 0
}

main "$@"