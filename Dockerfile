# 多阶段构建：先编译 release 二进制，再放进极小的运行镜像
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
# 运行期只需二进制，无需 Rust 工具链
COPY --from=build /src/target/release/nekosearch /usr/local/bin/nekosearch
EXPOSE 7700 7800 7900
ENTRYPOINT ["nekosearch"]
# 默认单机全角色
CMD ["--role", "all"]
