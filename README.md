# Aegis Vault

Aegis Vault 是一个用 Rust 编写的桌面加密保险箱应用，用来把文件或文件夹导入到加密仓库中，并在需要时再安全导出。

## 核心能力

- 创建和打开加密保险箱
- 导入单个文件或整个文件夹
- 导出保险箱中的内容
- 重命名、移动和删除保险箱条目
- 更改保险箱密码
- 健康检查
- 清理孤立密文和查看恢复空间
- 支持长文件的分块加密存储
- 提供桌面界面和进度反馈

## 项目结构

- `crates/vault-core`：核心保险箱逻辑，包括加密、索引、健康检查、导入导出和错误处理
- `crates/desktop-app`：基于 `iced` 的桌面界面

## 运行方式

在项目根目录执行：

```bash
cargo run -p desktop-app
```

## 测试

项目包含核心逻辑测试和健康检查测试，可以执行：

```bash
cargo test
```

## Windows 首版打包

当前的 Windows 首版发布形式是便携版 zip。

在项目根目录执行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
```

脚本会自动完成这些事情：

- 用 release 模式构建桌面应用
- 生成 `dist/windows/Aegis-Vault-v<version>-win64/`
- 复制可执行文件和发布说明
- 生成 `SHA256SUMS.txt`
- 在同目录生成可分发的 `.zip`

## Windows 安装器打包

在保留便携版 zip 的同时，也可以生成 Inno Setup 安装器。

前提：

- 已安装 Inno Setup 6
- `ISCC.exe` 在默认安装路径，或通过环境变量 `INNO_SETUP_COMPILER` 指向编译器

在项目根目录执行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-installer.ps1
```

脚本会自动完成这些事情：

- 先执行便携版打包，确保安装器输入目录最新
- 使用 `packaging/windows/installer.iss` 构建安装器
- 使用 `assets/aegis-vault.ico` 作为安装器图标
- 在 `dist/windows/` 下生成 `Aegis-Vault-Setup-v<version>-win64.exe`

## 设计特点

- 使用 `XChaCha20-Poly1305` 保护内容密文
- 使用 `Argon2` 处理密码派生
- 保险箱索引和配置都以加密形式保存
- 导入和导出过程使用临时文件，尽量避免半成品损坏数据
- 对缺失密文、孤立密文和损坏索引有健康检查能力

## 说明

这个仓库目前没有现成的 Git 仓库连接信息，所以我先把文档写在本地。
如果你把 GitHub 仓库地址发给我，我可以继续帮你把这个文件上传到对应仓库。
