# FireHOL CIDR Blocklist Auto-Update

## 概述

此 GitHub Action 自动从 FireHOL 项目下载 Level 1 IPv4 网段黑名单（CIDR 格式），并清理注释后存储。

## 数据源

- **项目**: FireHOL IP Lists (https://github.com/ktsaou/blocklist-ipsets)
- **数据集**: firehol_level1.netset
- **URL**: https://raw.githubusercontent.com/ktsaou/blocklist-ipsets/master/firehol_level1.netset
- **更新频率**: 每日更新
- **数据类型**: IPv4 CIDR 网段列表

## 运行时间

Workflow 自动运行时间（UTC）：
- 05:30 UTC (北京时间 13:30)
- 11:30 UTC (北京时间 19:30)
- 17:30 UTC (北京时间 01:30)
- 23:30 UTC (北京时间 07:30)

**每天运行 4 次**，确保黑名单保持最新。

## 输出格式

### 输出文件
`Data/firehol_cidr.txt`

### 格式说明
纯文本格式，每行一个 CIDR 网段：
```
1.2.3.0/24
5.6.7.8/32
10.0.0.0/16
```

### 示例
```
103.224.182.0/24
103.227.76.0/24
103.243.107.0/24
104.131.30.0/24
104.192.3.0/24
```

## 数据清理

原始文件包含注释和元数据，Workflow 会自动清理：

**清理规则**：
- ✅ 移除所有注释行（以 `#` 开头）
- ✅ 移除空行
- ✅ 仅保留 CIDR 网段数据

**原始文件示例**：
```
# FireHOL Level 1 - IP addresses that have been detected...
#
# Source: https://example.com
#
103.224.182.0/24
103.227.76.0/24
```

**清理后文件**：
```
103.224.182.0/24
103.227.76.0/24
```

## 配置要求

### GitHub Secrets

在 GitHub 仓库中添加以下 secret：

| Secret Name | Value | 说明 | 是否必需 |
|------------|-------|------|---------|
| `GIT_TOKEN` | GitHub Personal Access Token | 用于提交更改 | ✅ 必需 |

### GitHub Token 权限

确保 `GIT_TOKEN` 具有以下权限：
- ✅ `repo` (完整仓库访问权限)
- ✅ `workflow` (更新 workflow 文件)

## 手动触发

可以在 GitHub Actions 页面手动触发更新：

1. 进入仓库的 Actions 标签
2. 选择 "FireHOL CIDR Blocklist Update"
3. 点击 "Run workflow"
4. 点击绿色 "Run workflow" 按钮

## Workflow 步骤

1. **📥 下载黑名单**
   - 从 GitHub 下载最新的 FireHOL Level 1 列表
   - 验证 HTTP 状态码和文件完整性

2. **🔄 清理和处理**
   - 移除注释行（以 # 开头）
   - 移除空行
   - 输出纯净的 CIDR 列表

3. **📊 验证输出**
   - 检查文件完整性
   - 显示统计信息
   - 展示网段大小分布（/32, /24, /16 等）

4. **📤 提交更改**
   - 自动提交更新到 main 分支
   - 使用时间戳标记提交信息

## 统计信息

每次运行会输出：
- 📈 总 CIDR 网段数
- 📁 文件大小
- 🌐 网段大小分布
  - `/32` (单个 IP)
  - `/24` 网络
  - `/16` 网络
  - 其他大小

### 网段大小说明
- **`/32`**: 单个 IP 地址
- **`/24`**: 256 个 IP 地址（一个 C 类网络）
- **`/16`**: 65,536 个 IP 地址（一个 B 类网络）

## 错误处理

### 常见问题

**1. 下载失败**
```
❌ Failed to download FireHOL blocklist with HTTP 404
```
**解决方案**:
- ✅ 检查 URL 是否正确
- ✅ 确认 FireHOL 项目仓库可访问
- ✅ 等待一段时间后重试

**2. 文件为空**
```
❌ Downloaded file is empty or missing
```
**解决方案**:
- ✅ 检查网络连接
- ✅ 确认源数据文件存在
- ✅ 手动触发重新下载

**3. Git Push 失败**
```
❌ Permission denied
```
**解决方案**: 检查 `GIT_TOKEN` 权限配置

### HTTP 状态码说明

| 状态码 | 含义 | 解决方案 |
|-------|------|---------|
| 200 | 成功 | 正常 |
| 404 | 文件不存在 | 检查 URL，确认源文件存在 |
| 403 | 访问被拒绝 | GitHub 可能限流，稍后重试 |
| 500 | 服务器错误 | GitHub 服务问题，稍后重试 |

## FireHOL Level 1 说明

**FireHOL Level 1** 是一个高质量的 IP 黑名单，包含：
- 已知的攻击源 IP
- 恶意软件 C&C 服务器
- 扫描器和爬虫
- 垃圾邮件发送者
- 暴力破解攻击源

**信任级别**: Level 1 是 FireHOL 项目中最高信任级别的列表，误报率极低。

## 数据使用建议

⚠️ **重要提示**：
1. 此黑名单仅供参考，建议结合其他安全措施使用
2. 定期检查 workflow 运行状态，确保正常更新
3. 在生产环境使用前，请充分测试对业务的影响
4. CIDR 网段可能包含大量 IP，使用时注意性能影响

## 许可证

此 workflow 遵循项目主许可证。FireHOL 数据使用需遵守其项目许可协议。

**FireHOL 许可**: 数据可免费用于任何目的，详见 https://github.com/ktsaou/blocklist-ipsets