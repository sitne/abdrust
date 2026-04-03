ARG PROJECT_NAME=abdrust

FROM node:22-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/tsconfig.json frontend/vite.config.ts frontend/index.html ./
COPY frontend/src ./src
RUN npm install && npm run build

FROM rust:1.85-bookworm AS backend
ARG PROJECT_NAME
WORKDIR /app
COPY backend ./backend
COPY --from=frontend /app/frontend/dist ./frontend/dist
RUN cd backend && cargo build --release -p ${PROJECT_NAME}

FROM debian:bookworm-slim
ARG PROJECT_NAME
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=backend /app/backend/target/release/${PROJECT_NAME} /usr/local/bin/${PROJECT_NAME}
COPY --from=backend /app/frontend/dist ./frontend/dist
EXPOSE 3000
CMD ["/usr/local/bin/abdrust"]
