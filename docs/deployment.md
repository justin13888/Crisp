# Deploying and operating crisp-server

This guide covers running `crisp-server` in production: the supported
topology, the protocol it speaks, secrets, the platform endorsement key,
backups, migrations, scaling, observability, and upgrades. It ends with
runbooks for the incidents that need one. For local development use
`just dev` and the development [`compose.yaml`](../compose.yaml) instead;
neither is safe on a reachable host, and neither is a production path.

## Topology

`crisp-server` is deployed **only as a container on Kubernetes, behind an
Ingress or a Gateway API gateway**. There is no other supported way to run a
public server: no host reverse proxy, no Compose stack, no systemd unit, and
no server exposed directly.

```
Internet ──TLS──▶ ingress controller ──HTTP/1.1──▶ Service crisp-server ──▶ pod :8080 ──▶ PostgreSQL
                  (terminates TLS)     (cluster      (ClusterIP)              │
                                        network)                      :9464 metrics ◀── in-cluster scraper
```

- **The ingress terminates TLS; the server never does.** The server has no
  TLS listener and reads no certificate, key, or TLS setting, and the image
  carries none. It speaks plaintext HTTP/1.1 on the pod network only
  ([Protocol](#protocol)).
- **One server replica**, rolled out with the `Recreate` strategy, so two
  pods never run at once. See [Scaling](#scaling) for why.
- **PostgreSQL outside the server's pod**: a managed instance (RDS, Cloud SQL,
  Neon, …) with `sslmode=require` in `CRISP_DATABASE_URL`, or a cluster run by
  an operator such as CloudNativePG. Use its point-in-time recovery alongside
  the logical [backups](#backups) below.
- **Only the ingress controller reaches the API port**, enforced by a
  NetworkPolicy, and only the metrics scraper reaches the metrics port
  ([First deployment](#first-deployment)).

What the ingress enforces and what the server does:

| Concern | Where | Why there |
| --- | --- | --- |
| TLS, certificates, `Strict-Transport-Security` | ingress | The server never terminates TLS and cannot know the client's scheme. |
| HTTP/2 and HTTP/3 to clients | ingress | The server speaks HTTP/1.1 to the ingress only ([Protocol](#protocol)). |
| Total time to read a request body | ingress | The ingress buffers or times the whole body; the server only bounds idle gaps. |
| Request body size | both | The ingress caps it a little above `CRISP_MAX_BUNDLE_BYTES` (16 MiB), so an oversized upload still reaches the server's documented `413` problem; the server's cap is the authority. |
| Request-head and body-idle timeouts, connection cap | both | The server's own limits (`CRISP_HEADER_TIMEOUT_SECS`, `CRISP_BODY_IDLE_TIMEOUT_SECS`, `CRISP_MAX_CONNECTIONS`) still hold if the ingress is misconfigured. |
| Rate budgets (`CRISP_LIMIT_*`) | server | They key on the account and username as well as the address, which only the server knows; the ingress supplies the client address through `CRISP_TRUSTED_PROXIES` ([Client addresses](#client-addresses)). |
| Who can reach the pod | NetworkPolicy | A forwarding header is believed only from the ingress, so nothing else may connect to the API port. |

What each choice defends against is in the [threat model](threat-model.md).

**Footguns this topology rules out.** Each of these breaks an assumption
above, and none is supported:

- exposing the server any way but through the ingress: a `NodePort` or
  `LoadBalancer` Service, `hostNetwork` or `hostPort`, a published container
  port on a host, or `kubectl port-forward --address 0.0.0.0`;
- binding `CRISP_BIND_ADDR` to `0.0.0.0` anywhere but a pod network (the
  code default is loopback; the image and the ConfigMap below set
  `0.0.0.0:8080`, which is right inside a pod, and only there);
- trusting a proxy range broader than the network the ingress connects from:
  `CRISP_TRUSTED_PROXIES` is the pod CIDR, never `10.0.0.0/8` or the like
  (a zero-length prefix is refused at startup);
- routing `/metrics` or the metrics port through the Ingress or Gateway;
- configuring the ingress to speak h2c or TLS to the backend;
- more than one replica, or a `RollingUpdate` that briefly runs two.

## Protocol

The ingress-to-server hop is **plaintext HTTP/1.1**, and nothing else is
supported or served: `crisp-server` builds kynos without its `http2` and
`tls` features, so the listener refuses a prior-knowledge HTTP/2 (h2c)
preface and holds no TLS code at all
([`server/src/routes.rs`](../server/src/routes.rs)). HTTP/2, and HTTP/3
where the controller offers it, are still negotiated with clients by the
ingress, which owns TLS.

Why HTTP/1.1 rather than h2c for the upstream hop:

- It is what every common ingress controller and Gateway API implementation
  speaks to a backend by default. h2c needs an explicit opt-in per Service
  (Traefik's `h2c` scheme, the `kubernetes.io/h2c` `appProtocol`, or an
  Envoy/HAProxy cluster option), and some controllers have no h2c upstream
  at all.
- It needs no per-Service configuration: the Service below declares
  `appProtocol: http` only to make the choice explicit.
- Keep-alive connection reuse gives the ingress the benefit h2c's
  multiplexing would on a low-latency in-cluster hop: the controller keeps a
  pool of warm connections to the pod.
- Bundle upload and download, sign-in, and search have no streaming or
  many-small-requests workload where h2c on the last hop pays for itself.

Each HTTP/1.1 connection carries one request at a time, so
`CRISP_MAX_CONNECTIONS` (default 1024) also bounds the requests in flight.

## Cluster prerequisites

- A Kubernetes cluster with a NetworkPolicy-enforcing CNI (Calico, Cilium, …).
- An ingress controller or Gateway API implementation with certificates
  (e.g. cert-manager with ACME), and a DNS name for the API.
- A container registry the cluster can pull from.
- PostgreSQL 17, reachable from the `crisp` namespace.
- Off-cluster storage for backups (object storage or another machine).
- A secret store for the platform seed outside the cluster (a password
  manager vault, a cloud secret manager, or an offline copy; see
  [Platform key custody](#platform-key-custody)), and Secrets encrypted at
  rest in the cluster.

## Secrets and configuration

The server is configured entirely through environment variables, in the
`crisp` namespace: the two secrets in a Secret, everything else in a
ConfigMap. The full variable list is in the
[README](../README.md#server-configuration).

**The Secret** holds `CRISP_DATABASE_URL` and `CRISP_PLATFORM_SEED` (and,
during a rotation, `CRISP_PLATFORM_RETIRED_SEEDS`). Create it from values read
off the terminal, so neither reaches shell history or a process's arguments:

```bash
kubectl create namespace crisp
read -rs seed   # the platform seed from your secret store (openssl rand -hex 32, generated once)
read -rs url    # postgresql://crisp:…@db.example.internal:5432/crisp?sslmode=require
kubectl -n crisp create secret generic crisp-server --from-env-file=<(
  printf 'CRISP_PLATFORM_SEED=%s\nCRISP_DATABASE_URL=%s\n' "$seed" "$url")
unset seed url
```

Restrict `get`/`list` on Secrets in the namespace to the operators who can
deploy (anyone who can read the Secret, or exec into the pod, holds the
seed), and keep Secrets encrypted at rest. A Secret synced from an external
manager (External Secrets Operator, the Secrets Store CSI driver) works the
same way, as long as it is named `crisp-server` and carries these keys.

**The ConfigMap** holds the rest. A public server sets at least these:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: crisp-server
  namespace: crisp
data:
  CRISP_BIND_ADDR: "0.0.0.0:8080"           # the pod network; the Service routes to it
  CRISP_ALLOWED_ORIGINS: "https://app.example.com"
  CRISP_TRUSTED_PROXIES: "10.244.0.0/16"     # the pod CIDR; see "Client addresses"
  CRISP_METRICS_BIND_ADDR: "0.0.0.0:9464"   # for the in-cluster scraper; see "Observability"
  CRISP_LOG_FORMAT: "json"
  # Moderation (docs/public-server-policy.md, §4); a public server sets all three:
  CRISP_ADMIN_USR_IDS: "usr1..."
  CRISP_ABUSE_CONTACT: "mailto:abuse@example.com"
  CRISP_TERMS_URL: "https://example.com/terms"
  # Optional, defaults shown in the README: CRISP_MAX_BUNDLE_BYTES,
  # CRISP_VERIFY_CONCURRENCY, CRISP_SESSION_TTL_SECS, CRISP_QUOTA_*,
  # CRISP_MAX_STORED_BYTES, CRISP_HEADER_TIMEOUT_SECS,
  # CRISP_BODY_IDLE_TIMEOUT_SECS, CRISP_MAX_CONNECTIONS, CRISP_DB_*,
  # CRISP_LIMIT_* (see "Rate limits" below), RUST_LOG.
```

Notes:

- `CRISP_ALLOWED_ORIGINS` is required in production. Leaving it empty allows
  any origin, which is a development convenience only.
- `CRISP_VERIFY_CONCURRENCY` defaults to the CPUs the container may use,
  which follows its CPU limit; set it to match the CPU request if the pod
  has no limit.
- A changed ConfigMap or Secret reaches the server only when the pod
  restarts: `kubectl -n crisp rollout restart deployment/crisp-server`.

### Rate limits

The server budgets its expensive operations and answers `429` with
`Retry-After` when one is spent. Each `CRISP_LIMIT_*` value is
`REQUESTS/SECONDS`; an unset or empty value keeps the default below.

| Variable | Default | Keyed by |
| --- | --- | --- |
| `CRISP_LIMIT_REGISTER` | `10/3600` | client IP |
| `CRISP_LIMIT_CHALLENGE` | `10/300` | submitted username |
| `CRISP_LIMIT_CHALLENGE_PEER` | `30/60` | client IP |
| `CRISP_LIMIT_LOGIN` | `30/60` | client IP |
| `CRISP_LIMIT_PUBLISH` | `60/3600` | authenticated account |
| `CRISP_LIMIT_QUERY` | `120/60` | client IP (browse and search share it) |

The per-username challenge budget and the per-account publish budget are
unaffected by the ingress and can usually keep their defaults. Counters live
in the pod's memory, so a restart resets them.

### Client addresses

The server sees the ingress controller connect from its pod's address, never
from the client. `CRISP_TRUSTED_PROXIES` names the networks whose forwarding
header is believed, so the client-IP budgets count the client the ingress
forwards in `X-Forwarded-For`. Controller pods are rescheduled onto new
addresses, so name the cluster's **pod CIDR**, not a single address:

```bash
# The per-node ranges; their common supernet is the pod CIDR (often 10.244.0.0/16).
kubectl get nodes -o jsonpath='{range .items[*]}{.spec.podCIDR}{"\n"}{end}'
```

Some CNIs allocate pod addresses from their own pools rather than the node's
`podCIDR` (Calico's IP pools, a cloud VPC CNI); use the range the CNI
reports. Trust nothing wider: the NetworkPolicy below keeps every other pod in
that range off the API port, and that is what makes believing the range safe.

Start-up logs the policy in force at INFO, as the `client addresses` line,
whose `trusted_proxies` field names the header and the networks (e.g.
`X-Forwarded-For from 10.244.0.0/16`). Left unset, nobody is trusted
(`trusted_proxies=nobody`), and start-up logs a WARN saying so when it reads
the configuration: every client-IP budget is then
one limit shared by all clients, so with the defaults the whole server
accepts 10 registrations an hour and 30 sign-ins a minute, and a single
client can spend them for everyone.

The ingress must write the client's address itself rather than pass a
client's header through (both controllers below do by default), and it must
see the client's address in the first place: behind a cloud load balancer,
set `externalTrafficPolicy: Local` on the controller's own Service, or accept
the PROXY protocol from the load balancer, or every client arrives as a node
address. The full trust model is in the
[README](../README.md#ingress-and-client-addresses).

## First deployment

**Build and push the image** from a release tag. The build context is the
repository root ([`server/Dockerfile`](../server/Dockerfile)); `--pull`
picks up base-image security fixes:

```bash
git checkout <release tag>
podman build --pull -f server/Dockerfile -t registry.example.com/crisp-server:1.2.3 .
podman push registry.example.com/crisp-server:1.2.3
```

(`docker build` and `docker push` work the same.) The image runs as the
unprivileged user `10001`, writes nothing to its filesystem, and listens on
8080 (the API) and, once `CRISP_METRICS_BIND_ADDR` binds it, 9464 (metrics).

**Apply the manifests** below in the `crisp` namespace, after the Secret and
ConfigMap above. Deploy by a pinned tag or digest, never `latest`.

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: crisp-server
  namespace: crisp
spec:
  replicas: 1                # exactly one; see "Scaling"
  strategy:
    type: Recreate           # never two pods at once, even during a rollout
  selector:
    matchLabels:
      app.kubernetes.io/name: crisp-server
  template:
    metadata:
      labels:
        app.kubernetes.io/name: crisp-server
    spec:
      automountServiceAccountToken: false
      # The server drains in-flight requests for up to 20 s after SIGTERM.
      terminationGracePeriodSeconds: 30
      securityContext:
        runAsNonRoot: true
        runAsUser: 10001
        seccompProfile:
          type: RuntimeDefault
      containers:
        - name: server
          image: registry.example.com/crisp-server:1.2.3
          envFrom:
            - secretRef:
                name: crisp-server
            - configMapRef:
                name: crisp-server
          ports:
            - name: http
              containerPort: 8080
            - name: metrics
              containerPort: 9464
          # Boot applies pending migrations before it binds, so give it time.
          startupProbe:
            httpGet: { path: /api/v1/health, port: http }
            periodSeconds: 5
            failureThreshold: 60
          # Liveness never touches PostgreSQL, so a database outage does not
          # restart the pod; readiness takes it out of the Service instead.
          livenessProbe:
            httpGet: { path: /api/v1/health, port: http }
            periodSeconds: 10
          readinessProbe:
            httpGet: { path: /api/v1/ready, port: http }
            periodSeconds: 5
          lifecycle:
            # Keeps serving while the endpoint removal reaches the ingress
            # (the sleep action needs Kubernetes 1.30 or later).
            preStop:
              sleep: { seconds: 5 }
          resources:
            requests: { cpu: "1", memory: 512Mi }
            limits: { memory: 1Gi }
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop: [ALL]
---
apiVersion: v1
kind: Service
metadata:
  name: crisp-server
  namespace: crisp
spec:
  type: ClusterIP            # never NodePort or LoadBalancer
  selector:
    app.kubernetes.io/name: crisp-server
  ports:
    - name: http             # the API only; metrics are never on this Service
      port: 80
      targetPort: http
      appProtocol: http      # plaintext HTTP/1.1, never h2c or TLS
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: crisp-server
  namespace: crisp
spec:
  podSelector:
    matchLabels:
      app.kubernetes.io/name: crisp-server
  policyTypes: [Ingress]
  ingress:
    # The API port, from the ingress controller's namespace only.
    - from:
        - namespaceSelector:
            matchLabels:
              kubernetes.io/metadata.name: traefik      # your controller's namespace
      ports:
        - port: http
    # The metrics port, from the scraper's namespace only.
    - from:
        - namespaceSelector:
            matchLabels:
              kubernetes.io/metadata.name: monitoring   # your Prometheus namespace
      ports:
        - port: metrics
```

The kubelet's probes come from the node, which most CNIs admit whatever the
policy says; if yours does not, add the node CIDR for the `http` port.

**Route the API through the ingress.** Either of these terminates TLS for
`api.example.com`, adds `Strict-Transport-Security`, and forwards plaintext
HTTP/1.1 to the Service. Allow a little more than `CRISP_MAX_BUNDLE_BYTES`
(16 MiB) so an oversized upload reaches the server's `413`
`application/problem+json` answer. Per-client connection limits and address
bans, where you want them, also belong in the ingress.

*Traefik* (an `Ingress` with Traefik middlewares; Traefik writes the client's
address into `X-Forwarded-For` and believes no client-sent forwarding header
unless told to):

```yaml
apiVersion: traefik.io/v1alpha1
kind: Middleware
metadata:
  name: crisp-hsts
  namespace: crisp
spec:
  headers:
    stsSeconds: 31536000
    stsIncludeSubdomains: true
---
apiVersion: traefik.io/v1alpha1
kind: Middleware
metadata:
  name: crisp-body
  namespace: crisp
spec:
  # Reads the whole body before forwarding it, and caps it at 17 MiB.
  buffering:
    maxRequestBodyBytes: 17825792
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: crisp-server
  namespace: crisp
  annotations:
    traefik.ingress.kubernetes.io/router.entrypoints: websecure
    traefik.ingress.kubernetes.io/router.tls: "true"
    traefik.ingress.kubernetes.io/router.middlewares: crisp-crisp-hsts@kubernetescrd,crisp-crisp-body@kubernetescrd
spec:
  ingressClassName: traefik
  tls:
    - hosts: [api.example.com]
      secretName: api-example-com-tls    # e.g. issued by cert-manager
  rules:
    - host: api.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: crisp-server
                port:
                  name: http
```

Bound the total time to read a request on the `websecure` entry point in
Traefik's static configuration, e.g.
`--entryPoints.websecure.transport.respondingTimeouts.readTimeout=120s`.

*Gateway API* (any conformant implementation; a backend is spoken to over
HTTP/1.1 unless its Service port's `appProtocol` asks for h2c, which the
Service above does not):

```yaml
apiVersion: gateway.networking.k8s.io/v1
kind: Gateway
metadata:
  name: public
  namespace: gateway
spec:
  gatewayClassName: <your implementation>
  listeners:
    - name: https
      protocol: HTTPS
      port: 443
      hostname: api.example.com
      tls:
        mode: Terminate
        certificateRefs:
          - name: api-example-com-tls
      allowedRoutes:
        namespaces:
          from: Selector
          selector:
            matchLabels:
              kubernetes.io/metadata.name: crisp
---
apiVersion: gateway.networking.k8s.io/v1
kind: HTTPRoute
metadata:
  name: crisp-server
  namespace: crisp
spec:
  parentRefs:
    - name: public
      namespace: gateway
      sectionName: https
  hostnames: [api.example.com]
  rules:
    - backendRefs:
        - name: crisp-server
          port: 80
      timeouts:
        request: 120s
      filters:
        - type: ResponseHeaderModifier
          responseHeaderModifier:
            set:
              - name: Strict-Transport-Security
                value: max-age=31536000; includeSubDomains
```

Gateway API has no portable body-size limit: set it with your
implementation's own policy resource. The server's own cap still answers
`413` without one. With Gateway API, the NetworkPolicy's API rule names the
namespace the implementation's data-plane pods run in.

**Check it.** Wait for the rollout, then read `server-info` through a
loopback port-forward and through the ingress:

```bash
kubectl -n crisp rollout status deployment/crisp-server
kubectl -n crisp port-forward deployment/crisp-server 8080:8080 &   # loopback only
curl -fsS http://127.0.0.1:8080/api/v1/server-info
kill %1
curl -fsS https://api.example.com/api/v1/server-info
kubectl -n crisp logs deployment/crisp-server | grep -e 'client addresses' -e 'CRISP_TRUSTED_PROXIES'
```

Record the platform key's `usr_id` from `server-info` next to the seed in your
secret store: it is how you later confirm a restored seed is the right one.
The `client addresses` line must name the pod CIDR, and the log must hold no
`CRISP_TRUSTED_PROXIES is unset` WARN.

The image also carries a `HEALTHCHECK` against `GET /api/v1/health`, for
running it locally with `podman run` or `docker run`; Kubernetes ignores it
and uses the probes above.

## Serving the web client

The web client is static files: `cd client && bun run build` writes them to
`client/dist/` (run `just wasm` first, so the engine is included). Serve
them from their own origin, e.g. `app.example.com`, the origin you list in
`CRISP_ALLOWED_ORIGINS`. Any path that is not a file serves `index.html`,
because routes such as `/settings` are resolved in the page.

**Content-Security-Policy.** The session token and the unlocked identity key
live in the client's origin, so the client runs under a policy that lets
script, styles, workers, and the WASM engine come only from that origin
(threat model, [Security review](threat-model.md#security-review)). The
build puts the policy in a `<meta>` tag in `dist/index.html`, so it holds on
any static host. A `<meta>` policy cannot carry `frame-ancestors`, so the
host also sends the policy as a header with `frame-ancestors 'none'` added,
which stops another site from framing the app to trick a cook into clicking
through it. Both copies are enforced, so they must allow the same things.
The policy is defined once, as `CONTENT_SECURITY_POLICY` and `HEADER_ONLY` in
[`client/rsbuild.config.ts`](../client/rsbuild.config.ts), which is what
`bun run preview` sends; copy the header from there when it changes:

```
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' blob: data:; font-src 'self'; connect-src 'self' https: http://localhost:* http://127.0.0.1:*; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'
```

`connect-src` allows any `https:` server, because the cook chooses which
Crisp servers to use, plus a server on the cook's own machine (the client's
default is `http://localhost:8080`). A plain-`http` server anywhere else is
refused; a page served over `https` could not reach one anyway.

**Caddy:**

```caddy
app.example.com {
	root * /srv/crisp/client/dist
	try_files {path} /index.html
	file_server
	header {
		Strict-Transport-Security "max-age=31536000; includeSubDomains"
		Content-Security-Policy "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' blob: data:; font-src 'self'; connect-src 'self' https: http://localhost:* http://127.0.0.1:*; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
	}
}
```

**nginx** (inside the `server` block for the client's name):

```nginx
root /srv/crisp/client/dist;
location / {
    try_files $uri /index.html;
}
add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
add_header Content-Security-Policy "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' blob: data:; font-src 'self'; connect-src 'self' https: http://localhost:* http://127.0.0.1:*; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'" always;
```

Check it after deploying: `curl -sI https://app.example.com/ | grep -i content-security-policy`
prints the header, and the browser console on the app shows no
`Content-Security-Policy` errors while you unlock a key and connect to the
server.

## Platform key custody

`CRISP_PLATFORM_SEED` is the 32-byte seed from which the server derives its
Composite ML-DSA endorsement key ([`server/src/platform.rs`](../server/src/platform.rs)).
Every published bundle carries an endorsement signed with it, clients pin its
public key set per server on first contact (spec §8.5), and its `usr…` id is
the audience every login signature is bound to (spec §8.7). Treat the seed as
the server's identity.

**Generate** it once with a CSPRNG on a trusted machine:
`openssl rand -hex 32`. Never derive it from a password, never reuse one across
servers, and never copy one from an example or a development `.env`.

**Store** it in exactly two places:

1. the `crisp-server` Secret in the `crisp` namespace (encrypted at rest,
   readable only by the operators who can deploy), and
2. a secret store outside the cluster — a cloud secret manager or
   password-manager vault with access limited to the operators who can
   deploy, or a printed copy in a safe.

Never put it in a repository, a manifest, a ConfigMap, a CI variable that
untrusted branches can read, a chat, a ticket, a log, or a database backup.
The server never logs it: its `Debug` output omits the secret half.
Retired seeds in `CRISP_PLATFORM_RETIRED_SEEDS` are secrets with the same
custody for as long as they are listed.

**Back it up** separately from the database. A database backup without the
seed restores a server that cannot sign as itself; a seed without a database
backup is still the server's identity. Check the off-cluster copy after every
change by deriving the `usr_id` (start a throwaway server with it, or compare
with `server-info`) and matching it against the recorded value.

**Access**: only the people who can deploy the server can read the seed.
Rotate their access, not the seed, when someone leaves, unless they could have
copied it (then follow [Platform key compromise](#runbook-platform-key-compromise)).

### Rotation policy

The server holds **one current key**, published first in `server-info` with
`valid_until` unset, and optionally **retired keys** listed in
`CRISP_PLATFORM_RETIRED_SEEDS` as `SEED_HEX@VALID_UNTIL` entries. Each retired
key is published after the current key with its `valid_until`, keeps the
endorsements it made up to that instant verifying, and, with the current key,
signs the published key set (`key_set_signatures`). A client that pinned the
old key as current accepts a set that key signed without asking the cook
(spec §8.5); logins move to the new key's `usr…` id at once, and existing
sessions keep working because a session token does not depend on the key.

A **graceful rotation** — scheduled, or because the custody of a seed that has
not leaked is ending:

1. Generate a new seed (`openssl rand -hex 32`) and store it as the custody
   section says.
2. In the Secret, move the old seed into `CRISP_PLATFORM_RETIRED_SEEDS` with
   `@VALID_UNTIL` set to the current Unix time (`date +%s`), and set
   `CRISP_PLATFORM_SEED` to the new seed. Keep any earlier retired entries
   (comma-separated, in `retired` below):

   ```bash
   read -rs old; read -rs new; retired="$old@$(date +%s)"
   kubectl -n crisp patch secret crisp-server --type merge --patch-file <(printf \
     '{"stringData":{"CRISP_PLATFORM_SEED":"%s","CRISP_PLATFORM_RETIRED_SEEDS":"%s"}}' \
     "$new" "$retired")
   unset old new retired
   ```

3. Restart: `kubectl -n crisp rollout restart deployment/crisp-server`, then
   `kubectl -n crisp rollout status deployment/crisp-server`.
4. Check `server-info`: the new `usr_id` first with no `valid_until`, the old
   one after it with the instant from step 2, and one key-set signature per key.
5. Keep the retired entry for as long as clients may still connect for the
   first time since the rotation: a client accepts the new key only from a set
   its pinned key signed. A client that never saw the signed set before the
   entry is removed sees a key mismatch, as after a compromise. Six months is
   a reasonable default.
6. Then remove the entry. Clients that already updated keep the retired key
   pinned, so its endorsements keep verifying there; clients that first
   connect afterwards do not trust it, and authors republish to get an
   endorsement by the current key.

Endorsements stay valid across a graceful rotation, so rotate on a schedule
if you like (yearly is ample) — but never put a seed that may have leaked in
`CRISP_PLATFORM_RETIRED_SEEDS`: a retired key can still sign key sets, and
its holder can backdate endorsements into its window. A compromise follows the
runbook below instead.

## Backups

Bundles are stored inline in PostgreSQL (`BYTEA`), so the database is the
whole server state and its backup size grows with published content (up to
`CRISP_MAX_BUNDLE_BYTES` per revision). The platform seed is backed up
separately (above).

Take a logical backup daily with `pg_dump`'s custom format, which is
compressed and restores selectively. The server's image has no PostgreSQL
client, so run one in a throwaway pod in the namespace. This shell function,
used by the backup and restore below, starts one that reads
`CRISP_DATABASE_URL` from the Secret; Kubernetes expands `$(CRISP_DATABASE_URL)`
inside the pod, so the password never reaches the command line or the pod's
spec:

```bash
# crisp_pg PROGRAM [ARG...]: runs a PostgreSQL 17 client in the namespace
# $CRISP_NS (default crisp), its standard input and output attached here.
crisp_pg() {
  local command
  command=$(printf '"%s",' "$@"); command="[${command%,}]"
  kubectl -n "${CRISP_NS:-crisp}" run crisp-pg --rm -i --quiet --restart=Never \
    --image=postgres:17-alpine --overrides='{"apiVersion":"v1","spec":{
      "securityContext":{"runAsNonRoot":true,"runAsUser":70,"seccompProfile":{"type":"RuntimeDefault"}},
      "containers":[{"name":"crisp-pg","image":"postgres:17-alpine",
        "stdin":true,"stdinOnce":true,"command":'"$command"',
        "envFrom":[{"secretRef":{"name":"crisp-server"}}],
        "securityContext":{"allowPrivilegeEscalation":false,"capabilities":{"drop":["ALL"]}}}]}}'
}
```

```bash
#!/usr/bin/env bash
# crisp-backup — run daily from a machine with access to the crisp namespace
# and a PostgreSQL client of its own (for the pg_restore --list check).
set -euo pipefail
source crisp-pg.sh            # the crisp_pg function above
stamp=$(date -u +%Y%m%dT%H%M%SZ)
out=/var/backups/crisp/crisp-$stamp.dump
install -d -m 0700 /var/backups/crisp
crisp_pg pg_dump '--dbname=$(CRISP_DATABASE_URL)' --format=custom > "$out.partial"
pg_restore --list "$out.partial" > /dev/null   # a truncated dump fails here
mv "$out.partial" "$out"
# Ship off-cluster (encrypted at rest there), then prune local copies.
# rclone copy "$out" remote:crisp-backups/
find /var/backups/crisp -name 'crisp-*.dump' -mtime +7 -delete
```

Retention: keep 7 daily, 4 weekly, and 6 monthly backups off-cluster.
Encrypt them at rest and restrict who can read them: they hold every
account's public keys, every session's token digest, and every published
bundle.

`pg_dump` takes a consistent snapshot without stopping the server. For
recovery points finer than a day, use the managed provider's or the
operator's point-in-time recovery (WAL archiving).

**Test restores** at least quarterly, and after every PostgreSQL major
upgrade, by running [Database restore](#runbook-database-restore) against a
scratch database and a scratch namespace, never production: create a
`crisp-restore-test` namespace with its own Secret whose
`CRISP_DATABASE_URL` names the scratch database, run the manifests there
without the Ingress, and run the runbook with `CRISP_NS=crisp-restore-test`
exported and `-n crisp-restore-test` in place of `-n crisp` in **every**
command (without it, the commands act on production). Check `server-info` and a known bundle there
through a port-forward, then delete the namespace.

## Migrations

The schema is created and upgraded by SeaORM migrations
([`server/src/db.rs`](../server/src/db.rs)), which the server applies at boot
before it serves anything ([`server/src/lib.rs`](../server/src/lib.rs)).
Startup logs `connected to PostgreSQL; applying migrations`, and a failed
migration exits the process before it binds, so the startup probe never
passes and the pod restarts with the kubelet's backoff.

The stance:

- **Migrations run at boot.** With one replica and the `Recreate` strategy no
  two pods ever run them at once, and a release cannot run against a schema
  it does not expect.
- **Migrations are forward-only in production.** A migration's `down` exists
  for development; production rolls back by restoring the pre-upgrade backup
  ([Bad migration](#runbook-bad-migration)).
- **Back up before every upgrade** that ships a migration. Release notes say
  when one does; `git diff <old>..<new> -- server/src/db.rs` shows it.
- **Prefer additive migrations** (new tables, nullable columns) so the
  previous release keeps working against the new schema, which keeps a
  rollback of the binary alone possible.

## Scaling

Run **one server replica** (`replicas: 1`) with the **`Recreate`** rollout
strategy, and no HorizontalPodAutoscaler. A single pod is the supported
configuration because:

- every replica applies migrations at boot, and SeaORM takes no lock, so
  replicas starting together race on the schema. A `RollingUpdate` starts
  the new pod before the old one stops, which is the same race;
- the rate budgets' counters live in each pod's memory, so two pods behind
  one Service would each grant the full budget;
- every replica runs the expired-session purge task. That alone is harmless
  (the purge is an idempotent `DELETE`), but it is wasted work.

`Recreate` costs a short gap at each rollout while the new pod migrates and
becomes ready; the ingress answers `503` meanwhile. Scale vertically: verification and
endorsement are CPU-bound, so raise the pod's CPU request (and
`CRISP_VERIFY_CONCURRENCY` with it) and memory first.

## Observability

**Logs.** The server writes human-readable `tracing` lines to stdout. Every
request is traced at INFO and correlated with a server-generated request id,
returned to the client in the `x-request-id` header, which ties a client
report to the server's lines. `RUST_LOG` sets the filter, e.g.
`RUST_LOG=info,crisp_server=debug`. Set `CRISP_LOG_FORMAT=json` to write one
JSON object per event instead (its fields, level, target, and current span),
so a log store can query `request_id`, `operation_id`, `status`, or `usr_id`
directly. The kubelet rotates container logs (by default 5 files of 10 MiB);
ship them to your log store with the cluster's log collector (Fluent Bit,
Vector, the provider's agent) for longer retention.

```bash
kubectl -n crisp logs -f deployment/crisp-server
```

**Health.** `GET /api/v1/health` is the liveness and startup probe and never
touches PostgreSQL; `GET /api/v1/ready` is the readiness probe and answers
`503` while the database is unreachable. `kubectl -n crisp get pods` shows
the result, and `kubectl -n crisp describe pod` the probe failures.

**Metrics.** The server serves Prometheus metrics (OpenMetrics text) at
`GET /metrics` on a listener of its own, `CRISP_METRICS_BIND_ADDR`
(default `127.0.0.1:9464`; `off` disables it). It is never served on the
API listener, so the ingress cannot expose it and it is not part of
`spec/openapi.json`. The listener has no authentication: in the pod, bind it
on `0.0.0.0:9464` (the ConfigMap above), keep it off the Service the Ingress
routes to, and let the NetworkPolicy admit only the scraper's namespace.
With the Prometheus Operator, scrape the pod's `metrics` port directly:

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PodMonitor
metadata:
  name: crisp-server
  namespace: crisp
spec:
  selector:
    matchLabels:
      app.kubernetes.io/name: crisp-server
  podMetricsEndpoints:
    - port: metrics
      path: /metrics
```

Never add the metrics port to an Ingress, a Gateway route, or a Service of
type `NodePort` or `LoadBalancer`. The series, all prefixed `crisp_`:

| Metric | Type | Labels | Meaning |
| --- | --- | --- | --- |
| `http_requests_total` | counter | `operation`, `status` | Responses sent, by OpenAPI `operationId` (`<unmatched>` for an unknown path) and status code |
| `http_request_duration_seconds` | histogram | `operation` | Time to produce the response head |
| `cpu_job_duration_seconds` | histogram | `job` | Signature verification and endorsement time on the bounded pool: `publish` (verify and endorse a bundle), `register`, `login` |
| `cpu_job_wait_seconds` | histogram | `job` | Time a job waited for a slot (`CRISP_VERIFY_CONCURRENCY`) |
| `db_pool_connections`, `db_pool_idle_connections`, `db_pool_max_connections` | gauge | | PostgreSQL pool occupancy, sampled at each scrape |
| `purge_runs_total` | counter | `outcome` (`ok`, `error`) | Background purge passes |
| `purged_rows_total` | counter | `kind` (`session`, `challenge`) | Expired rows the purge deleted |

Starting points for alerts:

```promql
# Error rate: share of responses that are 5xx, per operation.
sum by (operation) (rate(crisp_http_requests_total{status=~"5.."}[5m]))
  / sum by (operation) (rate(crisp_http_requests_total[5m]))
# p95 latency per operation.
histogram_quantile(0.95, sum by (operation, le) (rate(crisp_http_request_duration_seconds_bucket[5m])))
# Verification saturating its pool: uploads waiting for a slot.
histogram_quantile(0.95, sum by (le) (rate(crisp_cpu_job_wait_seconds_bucket{job="publish"}[5m])))
# Pool exhausted: every connection in use.
crisp_db_pool_connections - crisp_db_pool_idle_connections >= crisp_db_pool_max_connections
# The purge is failing.
increase(crisp_purge_runs_total{outcome="error"}[1h]) > 0
```

Alert also on pod restarts and readiness (`kube_pod_container_status_restarts_total`,
`kube_pod_status_ready` from kube-state-metrics), the pod's CPU and memory
against its requests, and the database's storage, which grows with content.

## Upgrades

**crisp-server:**

1. Read the release's changes; note any migration.
2. Take a backup ([Backups](#backups)).
3. `git checkout <new tag>`, then build on fresh base images, push, and roll
   the Deployment onto the new tag:

   ```bash
   podman build --pull -f server/Dockerfile -t registry.example.com/crisp-server:1.2.4 .
   podman push registry.example.com/crisp-server:1.2.4
   kubectl -n crisp set image deployment/crisp-server server=registry.example.com/crisp-server:1.2.4
   kubectl -n crisp rollout status deployment/crisp-server
   ```

   `build --pull` picks up base-image security fixes, so rebuild and roll
   out at least monthly even without a release. Keep the manifest you apply
   from in step with the tag you set.
4. Watch the logs (`kubectl -n crisp logs -f deployment/crisp-server`) for the
   migration and `platform endorsement key ready`, and check that the
   `usr_id` in `server-info` is unchanged.

A release also carries the ingredient index it was built with: upgrading the
server and the web client together is how a self-hosted deployment receives
new ingredients and names. There is no separate index download and no sync
with other servers ([ingredient-index.md](ingredient-index.md#6-suggestions-and-releases)).

**PostgreSQL:** minor versions arrive through the managed provider's or the
operator's maintenance. For a major version, follow its upgrade procedure, or
back up, create a database on the new version, point `CRISP_DATABASE_URL` at
it, and restore ([Database restore](#runbook-database-restore)). Move
`crisp_pg`'s image to the new major at the same time: `pg_dump` must be at
least the server's version.

## Runbooks

### Runbook: platform key compromise

The seed has leaked, or someone who could have copied it is no longer trusted.
An attacker holding it can endorse arbitrary bundles as this server and accept
login signatures bound to its id.

1. Generate a new seed (`openssl rand -hex 32`) and replace it in the
   off-cluster secret store and in the Secret (as in
   [Rotation policy](#rotation-policy), step 2, but setting
   `CRISP_PLATFORM_RETIRED_SEEDS` to the remaining trusted entries, or `""`).
   Keep the old seed offline for the investigation; do not delete it yet.
   Do **not** list the compromised seed in `CRISP_PLATFORM_RETIRED_SEEDS`
   (remove it if it is there): the compromised key must leave the trusted set
   (spec §8.4), and a set it signed proves nothing, since the attacker can sign
   one too. Keep other retired entries only if their seeds are not exposed.
2. Restart: `kubectl -n crisp rollout restart deployment/crisp-server`.
3. Record the new `usr_id` from `server-info`.
4. Announce the rotation through a channel users already trust, with the old
   and new `usr_id`, the time window of the compromise, and the instruction to
   accept the new key only if it matches the announced id. Clients show a key
   mismatch, because no key they pinned signed the new set, and replace their
   whole pin only when the cook chooses **Trust new keys**.
5. Bundles endorsed by the old key before the compromise remain authentic, but
   clients that replaced their pin stop verifying their endorsement. Users can
   republish them to get a new endorsement; the author signature is
   unaffected.
6. Review the access that exposed the seed, and record the incident.

### Runbook: platform key loss

The seed is gone from the cluster and from the secret store. Nothing can recover
it: the server must take a new key, and since no key clients pinned can sign
the new set, with the same client-side consequences as a compromise. Follow steps 1–5 of the compromise runbook, announcing a loss rather
than a compromise. Then fix the custody gap that allowed it.

### Runbook: database restore

Restore from a `pg_dump --format=custom` file after data loss or corruption,
or into a scratch project to test a backup.

1. Stop the server so nothing writes during the restore:
   `kubectl -n crisp scale deployment/crisp-server --replicas=0`
2. Copy the chosen dump to the machine you run `kubectl` on, and verify it
   lists: `pg_restore --list crisp-<stamp>.dump | head`.
3. Replace the database contents with the dump, through the `crisp_pg`
   function from [Backups](#backups):

   ```bash
   crisp_pg pg_restore '--dbname=$(CRISP_DATABASE_URL)' --clean --if-exists \
     --no-owner --exit-on-error < crisp-<stamp>.dump
   ```

4. Start the server: `kubectl -n crisp scale deployment/crisp-server --replicas=1`.
   It applies any migration newer than the dump at boot.
5. Check `kubectl -n crisp rollout status deployment/crisp-server` (ready),
   `server-info` (the `usr_id` you recorded), and that a recently published
   bundle downloads.
6. Everything published after the dump's timestamp is lost from the server;
   authors still hold their signed files and can republish them.

### Runbook: bad migration

A release's migration failed at boot, or succeeded but broke the service.

1. **Migration failed at boot:** the server exits before serving and the pod
   restarts in `CrashLoopBackOff`. On PostgreSQL, SeaORM runs the whole batch
   of pending migrations in one transaction, so a failure rolls back every
   migration of that boot, earlier ones of the same release included, and
   leaves the schema as the previous release left it. Roll back the image:
   `kubectl -n crisp rollout undo deployment/crisp-server`, then
   `kubectl -n crisp rollout status deployment/crisp-server`.
   If the old release fails to start against the schema, continue with step 2.
2. **Migration applied but the release is broken:** scale the server to zero,
   restore the backup taken before the upgrade
   ([Database restore](#runbook-database-restore)), roll the Deployment back
   to the previous image with `rollout undo`, and scale it to one.
   Publications since the backup are lost from the server, as in a restore.
3. File an issue with the server's logs from the failed boot, and do not retry
   the upgrade until a fixed release exists.
