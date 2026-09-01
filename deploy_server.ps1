$ErrorActionPreference = "Stop"

$KEY = "C:\path\to\your\key.pem"
$SERVER = "ubuntu@your-server-ip-or-domain.com"

Write-Host "===========================================" -ForegroundColor Cyan
Write-Host " RustVPN - 1-Click Server Deployment" -ForegroundColor Cyan
Write-Host "===========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Package the source code, ignoring the huge target/ folder
Write-Host "[1/3] Packaging source code..." -ForegroundColor Yellow
tar.exe -czf source.tar.gz --exclude=target --exclude=.git .

# 2. Upload to AWS
Write-Host "[2/3] Uploading to AWS server..." -ForegroundColor Yellow
scp -o StrictHostKeyChecking=no -i $KEY source.tar.gz "$($SERVER):/tmp/source.tar.gz"

# 3. Extract and Deploy remotely
Write-Host "[3/3] Running automated deployment on server..." -ForegroundColor Yellow
$RemoteCommand = @"
    mkdir -p /home/ubuntu/rust-vpn-lab
    tar -xzf /tmp/source.tar.gz -C /home/ubuntu/rust-vpn-lab
    chmod +x /home/ubuntu/rust-vpn-lab/install_server.sh
    /home/ubuntu/rust-vpn-lab/install_server.sh
"@

ssh -o StrictHostKeyChecking=no -i $KEY $SERVER $RemoteCommand

# Cleanup local tar
Remove-Item source.tar.gz -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "===========================================" -ForegroundColor Green
Write-Host " Deployment script finished successfully!" -ForegroundColor Green
Write-Host "===========================================" -ForegroundColor Green
pause
