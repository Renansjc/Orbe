[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

Write-Host '=== Conexões ativas ==='
$active = Get-NetConnectionProfile | Where-Object { $_.IPv4Connectivity -ne 'Disconnected' -or $_.IPv6Connectivity -ne 'Disconnected' }
$active | Format-Table Name, InterfaceAlias, NetworkCategory, IPv4Connectivity, IPv6Connectivity -AutoSize

Write-Host "`n=== IPv6 (necessário para Matter local) ==="
Get-NetAdapterBinding -ComponentID ms_tcpip6 | Where-Object Enabled | Format-Table Name, DisplayName, Enabled -AutoSize

Write-Host "`n=== Firewall ==="
Get-NetFirewallProfile | Format-Table Name, Enabled, DefaultInboundAction -AutoSize

Write-Host "`nVerificações:"
Write-Host '1. O PC e o MT-01 devem estar na mesma rede local/VLAN.'
Write-Host '2. A rede do Windows deve ser Privada, não Pública.'
Write-Host '3. IPv6 deve permanecer ativo no adaptador usado.'
Write-Host '4. Gere um novo código em “Compartilhar via gateway Matter” e execute o pareamento em seguida.'
Write-Host '5. Não use rede de convidados, isolamento de clientes Wi-Fi ou VPN durante o teste.'
