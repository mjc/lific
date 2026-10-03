# You do not need Docker to run Lific. The project ships as a single binary
# with SQLite bundled (see the README for install options). This Dockerfile
# exists because MCP directory indexers (Glama and friends) build servers from
# a Dockerfile to verify them before listing; without one, Lific is invisible
# in their search. It also works fine if a container is genuinely how you want
# to deploy.
#
#   docker build -t lific .
#   docker run -p 3456:3456 -v lific-data:/data \
#     -e LIFIC_INIT_ADMIN_NAME="Your Name" \
#     -e LIFIC_INIT_ADMIN_PASSWORD="a long password" \
#     lific
#
# The database lives at /data/lific.db; mount a volume there to persist it.
# The first boot creates and migrates it, because the CMD below passes
# --init-if-missing (only `lific init` creates a database otherwise, and a
# container has nowhere to run that).
#
# Creating that database REQUIRES two environment variables, read once and
# never logged:
#
#   LIFIC_INIT_ADMIN_NAME      display name of the first admin
#   LIFIC_INIT_ADMIN_PASSWORD  its password
#
# Without both, the container refuses to start rather than creating an
# instance with no users: that instance would have signup open, would make
# whoever loaded the page first its administrator, and would mint and print
# an unbound operator API key into this log. An EXISTING database ignores
# both variables. Authentication stays required either way. First-boot init
# creates a database, it never opens the instance up.
#
# Recovering an instance that somehow has no administrator:
#
#   docker exec <container> lific --db /data/lific.db user create \
#     --username <name> --email <address> --password <password> --admin

# Topcoat renders and embeds its assets directly from the Rust source tree.
# The Debian release must match the runtime's glibc.
FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked
# Pre-create the data dir with the runtime UID; distroless has no shell to
# mkdir/chown with, and a VOLUME dir created at run time would be root-owned.
RUN mkdir /data && chown 65532:65532 /data

# Runtime. distroless/cc = glibc + CA certs + nothing else.
FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /src/target/release/lific /usr/local/bin/lific
COPY --from=build --chown=65532:65532 /data /data
VOLUME /data
EXPOSE 3456
ENTRYPOINT ["/usr/local/bin/lific", "--db", "/data/lific.db"]
# --init-if-missing: create + migrate /data/lific.db on first boot, and seed
# the first admin from LIFIC_INIT_ADMIN_NAME/LIFIC_INIT_ADMIN_PASSWORD when
# both are set (see the header). A no-op on every later start.
CMD ["start", "--init-if-missing"]
