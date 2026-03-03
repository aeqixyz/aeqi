# Nginx & SSL Configuration

## Domains

| Domain | Service | Backend |
|--------|---------|---------|
| dev.api.algostaking.com | API (dev) | localhost:8082 |
| api.algostaking.com | API (prod) | localhost:8083 |
| dev.app.algostaking.com | Frontend (dev) | /var/www/algostaking/dev/app |
| app.algostaking.com | Frontend (prod) | /var/www/algostaking/prod/app |
| dev.grafana.algostaking.com | Grafana (dev) | localhost:3000 |
| grafana.algostaking.com | Grafana (prod) | localhost:3002 |

## Config Files

Location: `/etc/nginx/sites-available/`

```bash
ls /etc/nginx/sites-available/
# algostaking-api.conf
# algostaking-app.conf
# algostaking-grafana-dev.conf
# algostaking-grafana-prod.conf
# etc.
```

## Common Operations

### Test Config
```bash
sudo nginx -t
```

### Reload (after config change)
```bash
sudo systemctl reload nginx
```

### View Logs
```bash
sudo tail -f /var/log/nginx/access.log
sudo tail -f /var/log/nginx/error.log
```

### Add New Site
```bash
# 1. Create config
sudo nano /etc/nginx/sites-available/mysite.conf

# 2. Enable site
sudo ln -s /etc/nginx/sites-available/mysite.conf /etc/nginx/sites-enabled/

# 3. Test and reload
sudo nginx -t && sudo systemctl reload nginx
```

## SSL with Certbot

### Add SSL to New Domain
```bash
# Ensure DNS A record points to server first
host example.algostaking.com  # Should return 5.9.83.245

# Get certificate
sudo certbot --nginx -d example.algostaking.com
```

### Renew Certificates
```bash
# Test renewal
sudo certbot renew --dry-run

# Force renewal
sudo certbot renew --force-renewal
```

### List Certificates
```bash
sudo certbot certificates
```

## Proxy Template

```nginx
server {
    server_name example.algostaking.com;

    location / {
        proxy_pass http://127.0.0.1:PORT;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    listen 80;
}
# Certbot will add SSL config automatically
```

## Server IP
- IPv4: 5.9.83.245
- IPv6: 2a01:4f8:162:310::2
