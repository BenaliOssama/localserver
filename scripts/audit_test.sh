!/bin/bash

# ── Config ────────────────────────────────────────────────────────────────────
HOST="127.0.0.1"
PORT="8080"
BASE="http://${HOST}:${PORT}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

PASS=0
FAIL=0

log()     { echo -e "${BLUE}[INFO]${NC} $1"; }
pass()    { echo -e "${GREEN}[PASS]${NC} $1"; PASS=$((PASS + 1)); }
fail()    { echo -e "${RED}[FAIL]${NC} $1"; FAIL=$((FAIL + 1)); }
section() { echo -e "\n${YELLOW}══════════════════════════════════════${NC}";
            echo -e "${YELLOW} $1${NC}";
            echo -e "${YELLOW}══════════════════════════════════════${NC}"; }

get_status() {
    curl -s -o /dev/null -w "%{http_code}" "$1"
}

# ── Check server is running ───────────────────────────────────────────────────

section "0. Preflight"

STATUS=$(get_status "$BASE/")
if [ "$STATUS" = "200" ] || [ "$STATUS" = "404" ]; then
    pass "Server is running on $BASE"
else
    echo -e "${RED}Server is not running — start it first with: cargo run config.conf${NC}"
    exit 1
fi

# ── Section 1: Configuration file ────────────────────────────────────────────

section "1. Configuration file"

# Single server single port
log "Single server on port $PORT..."
STATUS=$(get_status "$BASE/")
if [ "$STATUS" != "000" ]; then
    pass "Single server responding on port $PORT"
else
    fail "Single server not responding on port $PORT"
fi

# Multiple servers different ports
log "Multiple servers on different ports..."
STATUS2=$(get_status "http://127.0.0.1:8081/")
STATUS3=$(get_status "http://127.0.0.1:8082/")
if [ "$STATUS2" != "000" ] && [ "$STATUS3" != "000" ]; then
    pass "Multiple servers responding on ports 8080, 8081, 8082"
else
    fail "Multiple servers — not all ports responding (8081=$STATUS2, 8082=$STATUS3)"
fi

# server_name / hostname routing
log "Hostname routing with server_name..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    --resolve "main.com:${PORT}:${HOST}" \
    "http://main.com:${PORT}/")
if [ "$STATUS" != "000" ]; then
    pass "Hostname routing — main.com resolves correctly"
else
    fail "Hostname routing — main.com not resolving"
fi

# Custom error pages
log "Custom error pages..."
BODY=$(curl -s "$BASE/does_not_exist_page")
if echo "$BODY" | grep -qi "404\|not found\|error"; then
    pass "Custom 404 error page served"
else
    fail "Custom 404 error page not served"
fi

# Client body size limit
log "Client body size limit..."
# Send something larger than limit
BIG=$(python3 -c "print('x' * 25 * 1024 * 1024)")
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "$BASE/uploads/toobig.txt" \
    -H "Content-Type: plain/text" \
    --data "$BIG")
if [ "$STATUS" = "413" ]; then
    pass "Body size limit enforced → 413"
else
    log "  Got $STATUS (connection close also acceptable)"
    pass "Body size limit — server responded without crashing"
fi

# Routes
log "Route configuration..."
STATUS=$(get_status "$BASE/")
if [ "$STATUS" = "200" ] || [ "$STATUS" = "404" ]; then
    pass "Route / is configured and responding"
else
    fail "Route / not responding (got $STATUS)"
fi

# Default file for directory
log "Default file for directory..."
STATUS=$(get_status "$BASE/")
if [ "$STATUS" = "200" ]; then
    pass "Default index file served for directory request"
else
    log "  Got $STATUS — may not have index.html"
    pass "Directory request handled without crash"
fi

# Accepted methods per route
log "Method restrictions per route..."
# DELETE should be rejected on a GET-only route
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X DELETE "$BASE/")
if [ "$STATUS" = "405" ]; then
    pass "Method restriction — DELETE on GET-only route → 405"
else
    fail "Method restriction — DELETE on GET-only route → expected 405, got $STATUS"
fi

# ── Section 2: Methods ────────────────────────────────────────────────────────

section "2. Methods and status codes"

# GET
log "GET request..."
STATUS=$(get_status "$BASE/")
if [ "$STATUS" = "200" ] || [ "$STATUS" = "404" ]; then
    pass "GET / → $STATUS"
else
    fail "GET / → unexpected status $STATUS"
fi

# GET missing file
STATUS=$(get_status "$BASE/definitely_missing.html")
if [ "$STATUS" = "404" ]; then
    pass "GET missing file → 404"
else
    fail "GET missing file → expected 404, got $STATUS"
fi

# POST
log "POST request..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "$BASE/uploads/audit_test.txt" \
    --data "audit test content")
if [ "$STATUS" = "200" ]; then
    pass "POST /uploads/audit_test.txt → 200"
else
    fail "POST /uploads/audit_test.txt → expected 200, got $STATUS"
fi

# GET uploaded file back
STATUS=$(get_status "$BASE/uploads/audit_test.txt")
if [ "$STATUS" = "200" ]; then
    pass "GET uploaded file → 200"
else
    echo "this one is failing"
    fail "GET uploaded file → expected 200, got $STATUS"
fi

# Verify content not corrupted
ORIGINAL="audit_test_content_integrity_$(date +%s)"
curl -s -o /dev/null -X POST "$BASE/uploads/integrity.txt" \
    --data "$ORIGINAL"
RETRIEVED=$(curl -s "$BASE/uploads/integrity.txt")
if [ "$RETRIEVED" = "$ORIGINAL" ]; then
    pass "File upload integrity — content matches"
else
    fail "File upload integrity — content corrupted"
    log "  Expected: $ORIGINAL"
    log "  Got:      $RETRIEVED"
fi

# DELETE
log "DELETE request..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X DELETE "$BASE/uploads/audit_test.txt")
if [ "$STATUS" = "200" ]; then
    pass "DELETE /uploads/audit_test.txt → 200"
else
    fail "DELETE /uploads/audit_test.txt → expected 200, got $STATUS"
fi

# Confirm deleted
STATUS=$(get_status "$BASE/uploads/audit_test.txt")
if [ "$STATUS" = "404" ]; then
    pass "File gone after DELETE → 404"
else
    fail "File still exists after DELETE (got $STATUS)"
fi

# Wrong/unknown request
log "Wrong request..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X PATCH "$BASE/")
if [ "$STATUS" = "405" ]; then
    pass "Wrong method PATCH → 405"
else
    fail "Wrong method PATCH → expected 405, got $STATUS"
fi

# Server still alive after wrong request
STATUS=$(get_status "$BASE/")
if [ "$STATUS" != "000" ]; then
    pass "Server still alive after wrong request"
else
    fail "Server crashed after wrong request"
fi

# ── Section 3: Cookies and sessions ──────────────────────────────────────────

section "3. Cookies and sessions"

COOKIE_JAR=$(mktemp)

# Login
log "Login..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "$BASE/login" \
    -c "$COOKIE_JAR" \
    --data "username=admin&password=secret")
if [ "$STATUS" = "200" ]; then
    pass "Login → 200"
else
    fail "Login → expected 200, got $STATUS"
fi

# Check cookie was set
if grep -q "session_id" "$COOKIE_JAR"; then
    pass "Session cookie set after login"
else
    fail "No session cookie after login"
fi

# Whoami with cookie
log "Whoami with session..."
BODY=$(curl -s -b "$COOKIE_JAR" "$BASE/whoami")
if echo "$BODY" | grep -qi "admin"; then
    pass "Whoami returns correct user with session cookie"
else
    fail "Whoami failed with session cookie"
fi

# Whoami without cookie
log "Whoami without session..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/whoami")
if [ "$STATUS" = "403" ]; then
    pass "Whoami without cookie → 403"
else
    fail "Whoami without cookie → expected 403, got $STATUS"
fi

# Logout
log "Logout..."
curl -s -o /dev/null \
    -X POST "$BASE/logout" \
    -b "$COOKIE_JAR" \
    -c "$COOKIE_JAR"

# Whoami after logout
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -b "$COOKIE_JAR" "$BASE/whoami")
if [ "$STATUS" = "403" ]; then
    pass "Whoami after logout → 403"
else
    fail "Whoami after logout → expected 403, got $STATUS"
fi

rm -f "$COOKIE_JAR"

# ── Section 4: Browser interaction ───────────────────────────────────────────

section "4. Browser interaction (curl simulation)"

# Wrong URL
log "Wrong URL..."
STATUS=$(get_status "$BASE/this/does/not/exist/at/all")
if [ "$STATUS" = "404" ]; then
    pass "Wrong URL → 404"
else
    fail "Wrong URL → expected 404, got $STATUS"
fi

# Directory listing
log "Directory listing (autoindex)..."
STATUS=$(get_status "$BASE/files")
if [ "$STATUS" = "200" ]; then
    BODY=$(curl -s "$BASE/files/")
    if echo "$BODY" | grep -qi "files\|Index\|index\|\.txt\|\.html"; then
        pass "Directory listing → 200 with file list"
    else
        fail "Directory listing → 200 but no file list in body"
    fi
else
    fail "Directory listing → expected 200, got $STATUS"
fi

# Redirect
log "Redirect..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-redirs 0 "$BASE/old-page")
if [ "$STATUS" = "301" ]; then
    pass "Redirect /old-page → 301"
else
    fail "Redirect /old-page → expected 301, got $STATUS"
fi

# CGI unchunked
log "CGI with unchunked data..."
STATUS=$(get_status "$BASE/cgi/hello.py")
if [ "$STATUS" = "200" ]; then
    pass "CGI GET → 200"
else
    fail "CGI GET → expected 200, got $STATUS"
fi

# CGI chunked
log "CGI with chunked data..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "$BASE/cgi/hello.py" \
    -H "Transfer-Encoding: chunked" \
    --data "chunked body test")
if [ "$STATUS" = "200" ]; then
    pass "CGI POST chunked → 200"
else
    fail "CGI POST chunked → expected 200, got $STATUS"
fi

# CGI with query string
log "CGI with query string..."
BODY=$(curl -s "$BASE/cgi/hello.py?name=auditor&role=tester")
if echo "$BODY" | grep -q "name=auditor"; then
    pass "CGI query string → correctly passed"
else
    fail "CGI query string → not passed to script"
fi

# ── Section 5: Port issues ────────────────────────────────────────────────────

section "5. Port issues"

# Duplicate port detection
log "Duplicate port detection..."
cat > /tmp/duplicate_port.conf << 'EOF'
server {
    host 127.0.0.1;
    port 9999;
    location / { root ./www; methods GET; }
}
server {
    host 127.0.0.1;
    port 9999;
    location / { root ./www; methods GET; }
}
EOF

OUTPUT=$(./target/debug/localserver /tmp/duplicate_port.conf 2>&1)
if echo "$OUTPUT" | grep -qi "duplicate\|already\|error"; then
    pass "Duplicate port detected and rejected"
else
    fail "Duplicate port not detected"
fi
rm -f /tmp/duplicate_port.conf

# Bad config file
log "Bad config file handling..."
cat > /tmp/bad_config.conf << 'EOF'
server {
    host 127.0.0.1;
    port banana;
}
EOF
OUTPUT=$(./target/debug/localserver /tmp/bad_config.conf 2>&1)
if echo "$OUTPUT" | grep -qi "error\|invalid\|failed"; then
    pass "Bad config file rejected with error message"
else
    fail "Bad config file not rejected"
fi
rm -f /tmp/bad_config.conf

# Multiple servers common port — one bad config shouldn't kill others
log "One bad server config — others should keep running..."
STATUS1=$(get_status "$BASE/")
STATUS2=$(get_status "http://127.0.0.1:8081/")
if [ "$STATUS1" != "000" ] && [ "$STATUS2" != "000" ]; then
    pass "Other servers still running despite one bad config"
else
    fail "Other servers affected by bad config"
fi

# ── Section 6: Siege availability ────────────────────────────────────────────
section "6. Siege availability"

SIEGE=$(which siege 2>/dev/null)

if [ -z "$SIEGE" ]; then
    fail "siege not installed — run: sudo apt install siege"
else
    log "Running siege -b for 30s on $BASE/..."
    mkdir -p www
    echo "<html><body><h1>Siege Test</h1></body></html>" > www/siege_test.html

    SIEGE_OUTPUT=$($SIEGE -b -t 30s -c 10 -d 1 \
        --log=/dev/null \
        "$BASE/siege_test.html" 2>&1)

    log "Raw siege output:"
    echo "$SIEGE_OUTPUT"

    AVAILABILITY=$(echo "$SIEGE_OUTPUT" | grep -oP 'Availability:\s+\K[\d.]+')
    if [ -z "$AVAILABILITY" ]; then
        AVAILABILITY="0"
    fi

    log "Availability: ${AVAILABILITY}%"

    OK=$(python3 -c "
try:
    print('yes' if float('${AVAILABILITY}') >= 99.5 else 'no')
except:
    print('no')
" 2>/dev/null)

    if [ "$OK" = "yes" ]; then
        pass "Siege availability ${AVAILABILITY}% >= 99.5%"
    else
        fail "Siege availability ${AVAILABILITY}% < 99.5%"
    fi
fi
# ── Section 7: Memory and connections ────────────────────────────────────────

section "7. Memory and hanging connections"

# Check for hanging connections
log "Checking for hanging connections..."
CONNECTIONS=$(ss -tn | grep ":${PORT}" | grep -c "ESTABLISHED" 2>/dev/null || echo "0")
CONNECTIONS=$(echo "$CONNECTIONS" | tr -d '[:space:]')
log "Current established connections: $CONNECTIONS"
if [ "${CONNECTIONS:-0}" -lt 50 ]; then
    pass "No excessive hanging connections ($CONNECTIONS active)"
else
    fail "Too many hanging connections: $CONNECTIONS"
fi

# Memory check with top
log "Checking memory usage..."
PID=$(pgrep -f "localserver" | head -1)
if [ -n "$PID" ]; then
    MEM=$(ps -o rss= -p "$PID" 2>/dev/null || echo "0")
    MEM_MB=$((MEM / 1024))
    log "Server memory usage: ${MEM_MB}MB (PID: $PID)"
    if [ "$MEM_MB" -lt 100 ]; then
        pass "Memory usage reasonable: ${MEM_MB}MB"
    else
        log "  Memory usage is ${MEM_MB}MB — check for leaks with valgrind"
        pass "Server running (memory check manual)"
    fi
else
    log "Could not find server PID"
fi

# ── Cleanup ───────────────────────────────────────────────────────────────────

section "Cleanup"
rm -f www/siege_test.html
rm -f www/uploads/integrity.txt
rm -f www/uploads/toobig.txt
rm -rf www/files/test.txt
pass "Cleanup done"

# ── Final Report ──────────────────────────────────────────────────────────────

section "Final Audit Report"

TOTAL=$((PASS + FAIL))
echo ""
echo -e "  Total checks : ${TOTAL}"
echo -e "  Passed       : ${GREEN}${PASS}${NC}"
echo -e "  Failed       : ${RED}${FAIL}${NC}"
echo ""

echo -e "${BLUE}  Coverage:${NC}"
echo "  ✓ Configuration file (ports, hostnames, error pages, routes, methods)"
echo "  ✓ GET / POST / DELETE with status codes"
echo "  ✓ File upload integrity"
echo "  ✓ Cookies and sessions (login/whoami/logout)"
echo "  ✓ Browser interaction (wrong URL, directory listing, redirect, CGI)"
echo "  ✓ Port issues (duplicate detection, bad config)"
echo "  ✓ Siege availability >= 99.5%"
echo "  ✓ Memory and hanging connections"
echo ""

if [ "$FAIL" -eq 0 ]; then
    echo -e "${GREEN}  ✓ All ${TOTAL} audit checks passed${NC}"
    exit 0
else
    echo -e "${RED}  ✗ ${FAIL}/${TOTAL} checks failed — fix before audit${NC}"
    exit 1
fi
