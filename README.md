<p align="center"><img src="screenshots/logo.svg" alt="Flash" width="280"></p>

<p align="center">AI 帮你制作记忆卡片，Flash 帮你真正记住它们。</p>

<p align="center">
  <a href="https://flashmemorize.com">在线托管版本</a> ·
  <a href=".env.example">全部配置项</a> ·
  <a href="crates/flash-server/MCP.md">MCP 工具</a> ·
  <a href="CHANGELOG.md">更新日志</a> ·
  <a href="CONTRIBUTING.md">参与贡献</a>
</p>

![Flash 中的 AI 对话将八张卡片加入世界首都牌组，然后学习一张卡片：显示答案，再选择“记住”](screenshots/demo.gif)

一个可自行托管的间隔重复学习服务器。连接 ChatGPT、Claude 或任何 MCP
客户端：它会把卡片写入你的牌组并向你提问，Flash 使用
[FSRS](https://github.com/open-spaced-repetition/fsrs-rs) 为每次复习安排时间。
整个服务只有一个 Rust 二进制文件和一个数据目录，并支持导入和导出 Anki `.apkg` 牌组。

## 运行

```sh
docker run -d --name flash -p 127.0.0.1:8437:8437 -v flash-data:/data ghcr.io/flash-cards/flash
docker logs flash | grep enroll
```

打开 http://localhost:8437 中的注册链接，你会成为管理员；其他人可以从“设置”中邀请。
如果要从其他设备访问，请在前面配置 TLS 反向代理，并将 `FLASH_BASE_URL` 设置为代理地址
（[docker-compose.yml](docker-compose.yml) 提供了安全加固的配置）。数据保存在
`flash-data` 卷中，请定期备份；升级时拉取新镜像并重新创建容器即可。忘记密码时，运行
`docker exec flash flash-server reset-link you@example.com`，命令会输出设置新密码的链接
（使用 compose 时运行 `docker compose exec flash ...`）。

如果你像 0.3.0 及更早版本一样使用 `docker compose` 安装，数据位于该目录的卷中
（运行 `docker volume ls`，例如 `flash_flash-data`），而不是 `flash-data`。请先备份，
然后在同一目录升级，因为升级只能向前进行：

```sh
docker compose stop flash
docker run --rm -v flash_flash-data:/data -v "$PWD":/backup debian:bookworm-slim \
    tar czf /backup/flash-data.tgz -C /data .
docker compose pull && docker compose up -d
```

## 连接 AI

在 ChatGPT、Claude、Grok、Le Chat 或 Gemini 中，将
`https://<your host>/mcp` 添加为自定义连接器（使用 OAuth，无需复制密钥）。
也可以在终端运行：`claude mcp add --transport http flash https://<your host>/mcp`。
然后对它说：“请用我的牌组测试我。”

[在线托管版本](https://flashmemorize.com) 还支持从文件生成卡片，以及为口头回答评分。

## 许可证

[AGPL-3.0-only](LICENSE)；内置资源的许可说明见
[THIRD_PARTY.md](THIRD_PARTY.md)。欢迎在遵守 [CLA](CLA.md) 的前提下贡献代码。
