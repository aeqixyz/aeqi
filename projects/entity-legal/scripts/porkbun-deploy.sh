#!/usr/bin/env bash
set -euo pipefail

PORKBUN_API="https://api.porkbun.com/api/json/v3"
SERVER_IP="5.9.83.245"
DOMAIN="entity.legal"
DRY_RUN=false
VERBOSE=false

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

log()    { printf "${GREEN}[+]${RESET} %s\n" "$*"; }
warn()   { printf "${YELLOW}[!]${RESET} %s\n" "$*"; }
error()  { printf "${RED}[x]${RESET} %s\n" "$*" >&2; }
info()   { printf "${CYAN}[i]${RESET} %s\n" "$*"; }
header() { printf "\n${BOLD}=== %s ===${RESET}\n\n" "$*"; }

require_deps() {
    for cmd in curl jq dig; do
        command -v "$cmd" &>/dev/null || { error "$cmd is required but not installed"; exit 1; }
    done
}

require_credentials() {
    [[ -z "${PORKBUN_API_KEY:-}" ]] && { error "PORKBUN_API_KEY not set"; exit 1; }
    [[ -z "${PORKBUN_SECRET_KEY:-}" ]] && { error "PORKBUN_SECRET_KEY not set"; exit 1; }
}

auth_payload() {
    jq -n --arg ak "$PORKBUN_API_KEY" --arg sk "$PORKBUN_SECRET_KEY" \
        '{"apikey": $ak, "secretapikey": $sk}'
}

api_call() {
    local endpoint="$1"
    local data="${2:-$(auth_payload)}"
    local url="${PORKBUN_API}${endpoint}"

    if $VERBOSE; then
        info "POST $url"
    fi

    if $DRY_RUN && [[ "$endpoint" != "/ping" && "$endpoint" != /dns/retrieve* && "$endpoint" != /domain/listAll ]]; then
        warn "DRY RUN: POST $url"
        warn "DRY RUN: payload=$(echo "$data" | jq -c 'del(.secretapikey)')"
        echo '{"status":"SUCCESS","dry_run":true}'
        return 0
    fi

    local response http_code body
    response=$(curl -s -w "\n%{http_code}" -X POST "$url" \
        -H "Content-Type: application/json" \
        -d "$data" 2>&1)

    http_code=$(echo "$response" | tail -1)
    body=$(echo "$response" | sed '$d')

    if [[ "$http_code" -lt 200 || "$http_code" -ge 300 ]]; then
        error "HTTP $http_code from $endpoint"
        error "$body"
        return 1
    fi

    local status
    status=$(echo "$body" | jq -r '.status // "UNKNOWN"')
    if [[ "$status" != "SUCCESS" ]]; then
        local message
        message=$(echo "$body" | jq -r '.message // "No error message"')
        error "API error on $endpoint: $message"
        return 1
    fi

    echo "$body"
}

check_auth() {
    header "Authentication Check"
    local response
    response=$(api_call "/ping")
    local ip
    ip=$(echo "$response" | jq -r '.yourIp')
    log "Authenticated successfully"
    info "Server IP as seen by Porkbun: $ip"
}

list_domains() {
    header "Domain Listing"
    local response
    response=$(api_call "/domain/listAll")
    echo "$response" | jq -r '.domains[] | "\(.domain)\t\(.status)\t\(.expireDate)"' | column -t -s$'\t'
}

get_dns_records() {
    local domain="${1:-$DOMAIN}"
    header "DNS Records for $domain"
    local response
    response=$(api_call "/dns/retrieve/$domain")
    echo "$response" | jq -r '.records[] | "\(.id)\t\(.type)\t\(.name)\t\(.content)\t\(.ttl)"' | column -t -s$'\t'
    echo "$response"
}

get_dns_records_json() {
    local domain="${1:-$DOMAIN}"
    api_call "/dns/retrieve/$domain"
}

set_a_record() {
    local domain="${1:-$DOMAIN}"
    local ip="${2:-$SERVER_IP}"

    log "Setting A record: $domain -> $ip"

    local payload
    payload=$(jq -n \
        --arg ak "$PORKBUN_API_KEY" \
        --arg sk "$PORKBUN_SECRET_KEY" \
        --arg content "$ip" \
        '{
            "apikey": $ak,
            "secretapikey": $sk,
            "type": "A",
            "content": $content,
            "ttl": "600"
        }')

    local existing
    existing=$(api_call "/dns/retrieveByNameType/$domain/A" 2>/dev/null || true)
    local has_records
    has_records=$(echo "$existing" | jq -r '.records | length // 0' 2>/dev/null || echo "0")

    if [[ "$has_records" -gt 0 ]]; then
        info "Existing A record found, editing via editByNameType"
        local edit_payload
        edit_payload=$(jq -n \
            --arg ak "$PORKBUN_API_KEY" \
            --arg sk "$PORKBUN_SECRET_KEY" \
            --arg content "$ip" \
            '{
                "apikey": $ak,
                "secretapikey": $sk,
                "content": $content,
                "ttl": "600"
            }')
        api_call "/dns/editByNameType/$domain/A" "$edit_payload" >/dev/null
    else
        info "No existing A record, creating new"
        api_call "/dns/create/$domain" "$payload" >/dev/null
    fi

    log "A record set: $domain -> $ip"
}

set_cname() {
    local domain="${1:-$DOMAIN}"
    local subdomain="$2"
    local target="$3"

    log "Setting CNAME: $subdomain.$domain -> $target"

    local existing
    existing=$(api_call "/dns/retrieveByNameType/$domain/CNAME/$subdomain" 2>/dev/null || true)
    local has_records
    has_records=$(echo "$existing" | jq -r '.records | length // 0' 2>/dev/null || echo "0")

    if [[ "$has_records" -gt 0 ]]; then
        info "Existing CNAME found, editing"
        local edit_payload
        edit_payload=$(jq -n \
            --arg ak "$PORKBUN_API_KEY" \
            --arg sk "$PORKBUN_SECRET_KEY" \
            --arg content "$target" \
            '{
                "apikey": $ak,
                "secretapikey": $sk,
                "content": $content,
                "ttl": "600"
            }')
        api_call "/dns/editByNameType/$domain/CNAME/$subdomain" "$edit_payload" >/dev/null
    else
        local payload
        payload=$(jq -n \
            --arg ak "$PORKBUN_API_KEY" \
            --arg sk "$PORKBUN_SECRET_KEY" \
            --arg name "$subdomain" \
            --arg content "$target" \
            '{
                "apikey": $ak,
                "secretapikey": $sk,
                "name": $name,
                "type": "CNAME",
                "content": $content,
                "ttl": "600"
            }')
        api_call "/dns/create/$domain" "$payload" >/dev/null
    fi

    log "CNAME set: $subdomain.$domain -> $target"
}

set_txt_record() {
    local domain="${1:-$DOMAIN}"
    local value="$2"
    local subdomain="${3:-}"

    if [[ -n "$subdomain" ]]; then
        log "Setting TXT record: $subdomain.$domain -> $value"
    else
        log "Setting TXT record: $domain -> $value"
    fi

    local payload
    payload=$(jq -n \
        --arg ak "$PORKBUN_API_KEY" \
        --arg sk "$PORKBUN_SECRET_KEY" \
        --arg name "$subdomain" \
        --arg content "$value" \
        '{
            "apikey": $ak,
            "secretapikey": $sk,
            "name": $name,
            "type": "TXT",
            "content": $content,
            "ttl": "600"
        }')

    api_call "/dns/create/$domain" "$payload" >/dev/null

    if [[ -n "$subdomain" ]]; then
        log "TXT record created: $subdomain.$domain"
    else
        log "TXT record created: $domain"
    fi
}

delete_record() {
    local domain="${1:-$DOMAIN}"
    local record_id="$2"

    log "Deleting record $record_id from $domain"
    api_call "/dns/delete/$domain/$record_id" >/dev/null
    log "Record $record_id deleted"
}

delete_records_by_type() {
    local domain="${1:-$DOMAIN}"
    local record_type="$2"
    local subdomain="${3:-}"

    if [[ -n "$subdomain" ]]; then
        log "Deleting all $record_type records for $subdomain.$domain"
        api_call "/dns/deleteByNameType/$domain/$record_type/$subdomain" >/dev/null
    else
        log "Deleting all $record_type records for $domain"
        api_call "/dns/deleteByNameType/$domain/$record_type" >/dev/null
    fi
}

verify_dns_propagation() {
    local domain="$1"
    local expected_ip="$2"
    local max_attempts=12
    local wait_seconds=10

    info "Verifying DNS propagation for $domain (expecting $expected_ip)"

    for attempt in $(seq 1 $max_attempts); do
        local resolved
        resolved=$(dig +short "$domain" A @8.8.8.8 2>/dev/null | head -1)

        if [[ "$resolved" == "$expected_ip" ]]; then
            log "DNS propagated: $domain -> $resolved (attempt $attempt)"
            return 0
        fi

        if [[ "$attempt" -lt "$max_attempts" ]]; then
            info "Attempt $attempt/$max_attempts: got '$resolved', waiting ${wait_seconds}s..."
            sleep "$wait_seconds"
        fi
    done

    warn "DNS not yet propagated after $((max_attempts * wait_seconds))s"
    warn "Current resolution: $(dig +short "$domain" A @8.8.8.8 2>/dev/null | head -1)"
    warn "This is normal -- full propagation can take up to 48 hours"
    return 1
}

retrieve_ssl() {
    local domain="${1:-$DOMAIN}"
    header "SSL Certificate for $domain"
    local response
    response=$(api_call "/ssl/retrieve/$domain")
    local cert_preview
    cert_preview=$(echo "$response" | jq -r '.certificatechain' | head -3)
    log "Certificate retrieved"
    info "Chain preview: $cert_preview..."

    local output_dir="/tmp/porkbun-ssl-$domain"
    mkdir -p "$output_dir"
    echo "$response" | jq -r '.certificatechain' > "$output_dir/fullchain.pem"
    echo "$response" | jq -r '.privatekey' > "$output_dir/privkey.pem"
    echo "$response" | jq -r '.publickey' > "$output_dir/pubkey.pem"
    chmod 600 "$output_dir/privkey.pem"

    log "Certificates written to $output_dir/"
    info "  fullchain.pem  privkey.pem  pubkey.pem"
}

deploy_entity_legal() {
    header "Deploying entity.legal"

    info "Domain: $DOMAIN"
    info "Target IP: $SERVER_IP"
    info "Dry run: $DRY_RUN"
    echo ""

    log "Step 1/5: Verifying API credentials"
    check_auth
    echo ""

    log "Step 2/5: Recording existing DNS state"
    local existing_records
    existing_records=$(get_dns_records_json "$DOMAIN" 2>/dev/null || echo '{"records":[]}')
    local record_count
    record_count=$(echo "$existing_records" | jq '.records | length')
    info "Found $record_count existing records"
    if [[ "$record_count" -gt 0 ]]; then
        echo "$existing_records" | jq -r '.records[] | "  \(.type)\t\(.name)\t\(.content)"' | column -t -s$'\t'
    fi
    echo ""

    log "Step 3/5: Setting A record ($DOMAIN -> $SERVER_IP)"
    set_a_record "$DOMAIN" "$SERVER_IP"
    echo ""

    log "Step 4/5: Setting CNAME (www.$DOMAIN -> $DOMAIN)"
    set_cname "$DOMAIN" "www" "$DOMAIN"
    echo ""

    log "Step 5/5: Verifying DNS propagation"
    if verify_dns_propagation "$DOMAIN" "$SERVER_IP"; then
        echo ""
        header "Deployment Complete"
        log "$DOMAIN -> $SERVER_IP"
        log "www.$DOMAIN -> $DOMAIN (CNAME)"
        echo ""
        info "Next steps:"
        info "  1. Install nginx config: sudo cp nginx-entity-legal.conf /etc/nginx/sites-available/entity-legal"
        info "  2. Enable site: sudo ln -sf /etc/nginx/sites-available/entity-legal /etc/nginx/sites-enabled/"
        info "  3. Create web root: sudo mkdir -p /var/www/entity-legal"
        info "  4. Get SSL cert: sudo certbot --nginx -d entity.legal -d www.entity.legal"
        info "  5. Reload nginx: sudo nginx -t && sudo systemctl reload nginx"
    else
        echo ""
        header "Deployment Submitted"
        warn "DNS records are set but propagation is pending"
        warn "Re-run with 'verify' command to check propagation status"
    fi
}

deploy_with_extras() {
    deploy_entity_legal

    echo ""
    header "Additional Records"

    log "Setting SPF record"
    set_txt_record "$DOMAIN" "v=spf1 a mx ~all"

    log "Setting security contact"
    set_txt_record "$DOMAIN" "v=TLSRPTv1; rua=mailto:admin@entity.legal" "_smtp._tls"

    echo ""
    header "Final DNS State"
    get_dns_records "$DOMAIN" >/dev/null
    local final
    final=$(get_dns_records_json "$DOMAIN")
    echo "$final" | jq -r '.records[] | "\(.type)\t\(.name)\t\(.content)"' | column -t -s$'\t'
}

usage() {
    cat <<USAGE
${BOLD}porkbun-deploy.sh${RESET} -- DNS deployment for entity.legal

${BOLD}USAGE${RESET}
    porkbun-deploy.sh [OPTIONS] <COMMAND>

${BOLD}COMMANDS${RESET}
    deploy          Full deployment (A + CNAME + verify)
    deploy-full     Deploy + SPF + TLS reporting records
    auth            Test API credentials
    domains         List all domains on account
    records         Show DNS records for entity.legal
    verify          Check DNS propagation status
    ssl             Retrieve and save SSL certificate

    set-a <ip>              Set A record (default: $SERVER_IP)
    set-cname <sub> <tgt>   Set CNAME record
    set-txt <value> [sub]   Set TXT record
    delete <record_id>      Delete record by ID
    delete-type <type> [sub] Delete all records of type

${BOLD}OPTIONS${RESET}
    --dry-run       Show what would be done without executing
    --verbose       Show API request URLs
    --domain <d>    Override domain (default: $DOMAIN)
    --ip <ip>       Override target IP (default: $SERVER_IP)
    -h, --help      Show this help

${BOLD}ENVIRONMENT${RESET}
    PORKBUN_API_KEY     Your Porkbun API key (required)
    PORKBUN_SECRET_KEY  Your Porkbun secret API key (required)

${BOLD}EXAMPLES${RESET}
    export PORKBUN_API_KEY=pk1_xxx
    export PORKBUN_SECRET_KEY=sk1_xxx

    porkbun-deploy.sh auth
    porkbun-deploy.sh --dry-run deploy
    porkbun-deploy.sh deploy
    porkbun-deploy.sh set-txt "v=spf1 a mx ~all"
    porkbun-deploy.sh delete 12345678
USAGE
}

main() {
    require_deps

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dry-run)  DRY_RUN=true; shift ;;
            --verbose)  VERBOSE=true; shift ;;
            --domain)   DOMAIN="$2"; shift 2 ;;
            --ip)       SERVER_IP="$2"; shift 2 ;;
            -h|--help)  usage; exit 0 ;;
            *)          break ;;
        esac
    done

    local command="${1:-help}"
    shift || true

    if [[ "$command" == "help" ]]; then
        usage
        exit 0
    fi

    require_credentials

    if $DRY_RUN; then
        warn "DRY RUN MODE -- mutating operations will be simulated"
        echo ""
    fi

    case "$command" in
        deploy)
            deploy_entity_legal
            ;;
        deploy-full)
            deploy_with_extras
            ;;
        auth)
            check_auth
            ;;
        domains)
            list_domains
            ;;
        records)
            get_dns_records "$DOMAIN"
            ;;
        verify)
            verify_dns_propagation "$DOMAIN" "$SERVER_IP"
            ;;
        ssl)
            retrieve_ssl "$DOMAIN"
            ;;
        set-a)
            local ip="${1:-$SERVER_IP}"
            set_a_record "$DOMAIN" "$ip"
            ;;
        set-cname)
            [[ $# -lt 2 ]] && { error "Usage: set-cname <subdomain> <target>"; exit 1; }
            set_cname "$DOMAIN" "$1" "$2"
            ;;
        set-txt)
            [[ $# -lt 1 ]] && { error "Usage: set-txt <value> [subdomain]"; exit 1; }
            set_txt_record "$DOMAIN" "$1" "${2:-}"
            ;;
        delete)
            [[ $# -lt 1 ]] && { error "Usage: delete <record_id>"; exit 1; }
            delete_record "$DOMAIN" "$1"
            ;;
        delete-type)
            [[ $# -lt 1 ]] && { error "Usage: delete-type <type> [subdomain]"; exit 1; }
            delete_records_by_type "$DOMAIN" "$1" "${2:-}"
            ;;
        *)
            error "Unknown command: $command"
            usage
            exit 1
            ;;
    esac
}

main "$@"
