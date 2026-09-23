# Contract: House Authentik OIDC (E3)

**Status**: hireling application **captured** (verbatim fixtures below, probed
read-only 2026-09-23). The token-endpoint response shape remains **assumed**
(cannot be probed without completing an authenticated code exchange).

**Provenance**: the instance shape was first established from the live
application `chat` (same-day, pre-provisioning) — that fixture is preserved in
commit `d8ef5c9`. Provisioning (Josh-delegated, via the Authentik admin API,
2026-09-23) created provider pk 5 `hireling` (confidential client,
`sub_mode=user_uuid`, house signing key, strict redirect
`https://hireling.flinntech.com/api/auth/callback`) and application "Hireling",
slug `hireling`, launch_url `https://hireling.flinntech.com`. The fixtures below
replace the slug-substituted assumptions from `d8ef5c9`. Probing discipline:
read-only `GET`s only; Authentik is never written from this repo.

## 1. Captured fixture — discovery (verbatim)

`GET https://auth.flinntech.com/application/o/hireling/.well-known/openid-configuration`
→ 200, captured 2026-09-23 (re-captured after the §4 scope-mapping fix; the
initial capture, with `scopes_supported: ["openid"]` only, is in commit
`bcd2478`):

```json
{
    "issuer": "https://auth.flinntech.com/application/o/hireling/",
    "authorization_endpoint": "https://auth.flinntech.com/application/o/authorize/",
    "token_endpoint": "https://auth.flinntech.com/application/o/token/",
    "userinfo_endpoint": "https://auth.flinntech.com/application/o/userinfo/",
    "end_session_endpoint": "https://auth.flinntech.com/application/o/hireling/end-session/",
    "introspection_endpoint": "https://auth.flinntech.com/application/o/introspect/",
    "revocation_endpoint": "https://auth.flinntech.com/application/o/revoke/",
    "device_authorization_endpoint": "https://auth.flinntech.com/application/o/device/",
    "backchannel_logout_supported": true,
    "backchannel_logout_session_supported": true,
    "frontchannel_logout_supported": true,
    "frontchannel_logout_session_supported": true,
    "response_types_supported": [
        "code",
        "id_token",
        "id_token token",
        "code token",
        "code id_token",
        "code id_token token"
    ],
    "response_modes_supported": [
        "query",
        "fragment",
        "form_post"
    ],
    "jwks_uri": "https://auth.flinntech.com/application/o/hireling/jwks/",
    "grant_types_supported": [
        "authorization_code",
        "refresh_token",
        "implicit",
        "client_credentials",
        "password",
        "urn:ietf:params:oauth:grant-type:device_code"
    ],
    "id_token_signing_alg_values_supported": [
        "RS256"
    ],
    "subject_types_supported": [
        "public"
    ],
    "token_endpoint_auth_methods_supported": [
        "client_secret_post",
        "client_secret_basic"
    ],
    "acr_values_supported": [
        "goauthentik.io/providers/oauth2/default"
    ],
    "scopes_supported": [
        "openid",
        "email",
        "profile"
    ],
    "request_parameter_supported": false,
    "claims_supported": [
        "sub",
        "iss",
        "aud",
        "exp",
        "iat",
        "auth_time",
        "acr",
        "amr",
        "nonce",
        "email",
        "email_verified",
        "name",
        "given_name",
        "preferred_username",
        "nickname",
        "groups"
    ],
    "claims_parameter_supported": false,
    "code_challenge_methods_supported": [
        "plain",
        "S256"
    ]
}
```

## 2. Captured fixture — JWKS (verbatim)

`GET https://auth.flinntech.com/application/o/hireling/jwks/` → 200, captured
2026-09-23 (single signing key, public key material). Captured fact: `kid`,
modulus, and certificate are **identical to the `chat` application's key** — both
providers sign with the same house signing key, as provisioned.

```json
{
    "keys": [
        {
            "alg": "RS256",
            "kid": "xt9vw1ZDqiYU9AxKVkjQf7q2MJPe2pdAiz78soUjWNlOyTUwmfveMelAt5Wps6WukIEyshilrZaXoxDYdEBLLg",
            "kty": "RSA",
            "use": "sig",
            "n": "jhgGsaVcrMuMbgnUKF2CUDRv8VVKCu2IjQGIVV87Dlz-MLuLYs93TntvjG8ufI5zXSkL3-yXRpbkqrRFx1P0n2lTEs-EnJUA6MC6M5ppASz_RooebZZoI6DAdHAw1KTz1hdp0IhiQG2ToyuRtZMuTGGkb35wkcyrbncdDJvozlJXd2IsqvwtIsfd3Vz9pIvD_paQgN_AQ_6od4AwVWXt4HotIkV6m59GTJlPaQxa-hjkQagYgSD8TfABQKc26S4EjR__yJGAKFJhpzI1qC_DbH8ojPSodS20DIHuktidltONaNi1NG_rs4Se8E7oyGx7XFTm3wUt7_FjqOcFTrf_rDOH_BgByXyiqNfr4t70GcvRDypHPO_KGxaJ6mXdin_PXyMPqqpS6Ary5W07AsHE5VX6HXD2Ou0tqvXdGijbuusvxHOgqvcgSskZqSMqvb5mKCC1hkEHO3pwSGo-mmNyOT5K8zwpUWT6UAGEt9VHcJwTUN38wScBAU6C0YulDEHYJSKaFFOWA4zOqXOhnZDy4d307OF05mxEanOyuaWEYc2xsI1noAZ0z0bq49sfB_cZ8TvidOtHF7ooxo8oUnTpBaAJmavRJoiu7FH5wLCZ6fG10J0X67jkdjAIMMmTfty59iSO7FjwLx7w-LCMLmua79nBEbKMlEc3fHeNutV0Thk",
            "e": "AQAB",
            "x5c": [
                "MIIFUzCCAzugAwIBAgIRALXGbBoA0kD+uboh7KYWNu0wDQYJKoZIhvcNAQELBQAwHTEbMBkGA1UEAwwSYXV0aGVudGlrIDIwMjYuNS42MB4XDTI2MDgxMzIzNDYzMVoXDTI3MDgxNDIzNDYzMVowVjEqMCgGA1UEAwwhYXV0aGVudGlrIFNlbGYtc2lnbmVkIENlcnRpZmljYXRlMRIwEAYDVQQKDAlhdXRoZW50aWsxFDASBgNVBAsMC1NlbGYtc2lnbmVkMIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAjhgGsaVcrMuMbgnUKF2CUDRv8VVKCu2IjQGIVV87Dlz-MLuLYs93TntvjG8ufI5zXSkL3+yXRpbkqrRFx1P0n2lTEs+EnJUA6MC6M5ppASz/RooebZZoI6DAdHAw1KTz1hdp0IhiQG2ToyuRtZMuTGGkb35wkcyrbncdDJvozlJXd2IsqvwtIsfd3Vz9pIvD/paQgN/AQ/6od4AwVWXt4HotIkV6m59GTJlPaQxa+hjkQagYgSD8TfABQKc26S4EjR//yJGAKFJhpzI1qC/DbH8ojPSodS20DIHuktidltONaNi1NG/rs4Se8E7oyGx7XFTm3wUt7/FjqOcFTrf/rDOH/BgByXyiqNfr4t70GcvRDypHPO/KGxaJ6mXdin/PXyMPqqpS6Ary5W07AsHE5VX6HXD2Ou0tqvXdGijbuusvxHOgqvcgSskZqSMqvb5mKCC1hkEHO3pwSGo+mmNyOT5K8zwpUWT6UAGEt9VHcJwTUN38wScBAU6C0YulDEHYJSKaFFOWA4zOqXOhnZDy4d307OF05mxEanOyuaWEYc2xsI1noAZ0z0bq49sfB/cZ8TvidOtHF7ooxo8oUnTpBaAJmavRJoiu7FH5wLCZ6fG10J0X67jkdjAIMMmTfty59iSO7FjwLx7w+LCMLmua79nBEbKMlEc3fHeNutV0ThkCAwEAAaNVMFMwUQYDVR0RAQH/BEcwRYJDWlN0RjExUzRrQzhWSXVqUmxPYkhzZ2pVRE1TWlE5MGNTYVVHUlZoSy5zZWxmLXNpZ25lZC5nb2F1dGhlbnRpay5pbzANBgkqhkiG9w0BAQsFAAOCAgEAG66/4DzxtSAE+pMHirKblmHlf8uRrpH29LInfGQD+36juRsNJVVikPqUFDiWYDDEJnd7vysLE2g9Djry5528WoYnLoPqOZaF8mJivP3EC5ynqPuf9BFpqfO8dh0XuCQgE3CdPwf2YGqtMCV8BVAy/T6slMQzrennsaYIxpMyWUNHjIkhBmTEQ0mE7ea5FdBQYVPBbRIycDHIhszLtKJtB92Y7klBcB0u2WWYMjIVZ6LxO9GbGX89aHjIW1imM4fQP1ZhLJm1Ns36poLIwQwhJM9vvzRTWqpUSv1CV8nNfCXqJfzkVWPMkDoIW1mXUS1+NCDU7u0BKbxDyw7UyXUhKmcjEfsc7pt7sS7xdQjQlXEG2aTTaraADwzgCMQzkMmbaWl1i+QGxMlg9Bwn/UKqCCoKdDWhBIefuuml4qsj+ipDm9myrb1TvhtR1vGBzWbiRUslCTaopcs8JWq2RV84E+arOfRHXwjqMHVYUIcNVhX/vmnv1ydjxNfjUeO//JAolfgBcxkChguNrv6h/7wD8nCyxiSr965XuR5Ka0V6l97+SkIpPhDTtxz5VzTEOdWEakBWjkYVJ2KCsVP1YPOZGKqfZoSE8ErHn3FPSCHzaABvbzSimPns7izzJH3V1DnTiO+PI6xEeOsBvn/2fzbufIY03LmSlLlp1emHz2UkT+A="
            ],
            "x5t": "-l6k8EdqEsPw5hQYvn7fBEJw_WA",
            "x5t#S256": "99aKPw7cdb25IKb5fzH00GRZoT4TAsvbl5yXtKlEY_s"
        }
    ]
}
```

Instance version: Authentik **2026.5.6** (from the `x5c` certificate CN).

## 3. Contract facts the design relies on (captured)

| Fact | Value |
|---|---|
| Issuer (`HIRELING_OIDC_ISSUER`) | `https://auth.flinntech.com/application/o/hireling/` |
| Authorization endpoint | `https://auth.flinntech.com/application/o/authorize/` (shared across applications) |
| Token endpoint | `https://auth.flinntech.com/application/o/token/` (shared) |
| Token endpoint auth | `client_secret_basic`, `client_secret_post` → confidential client; design uses `client_secret_basic` |
| JWKS URI | `https://auth.flinntech.com/application/o/hireling/jwks/` |
| ID token signing | **RS256 only** |
| PKCE | `plain` and `S256`; design uses `S256` |
| Subject type | `public`; provider sub mode `user_uuid` → `sub` is the stable user UUID visible in the Authentik directory |
| Registered redirect URI | `https://hireling.flinntech.com/api/auth/callback` (strict match, as provisioned) |
| Client credentials | 1Password `op://vex-lab/authentik-mimir`, fields `hireling-oidc-client-id` / `hireling-oidc-client-secret` → deploy env `HIRELING_OIDC_CLIENT_ID` / `HIRELING_OIDC_CLIENT_SECRET`. Never in-repo. |
| End-session endpoint | `https://auth.flinntech.com/application/o/hireling/end-session/` — exists but **unused**: logout is local-only (design review decision; the house IdP session is shared with other apps) |

## 4. Resolved delta: profile scope mappings (recorded for the next provisioner)

As first provisioned, the provider advertised `scopes_supported: ["openid"]`
only — no `preferred_username` or `name` claims (initial capture in commit
`bcd2478`). **Resolved 2026-09-23**: the three default scope mappings
(`openid`, `profile`, `email`) were attached to provider pk 5 via the admin API
(Josh-delegated), and the discovery document re-captured in §1 now advertises
the full scope and claim sets. The design's claim mapping (§7) runs
claims-first per the spec's account-mapping Assumption.

**Why it happened**: providers created through the Authentik admin API do NOT
receive the scope mappings the UI wizard auto-attaches — an API-driven
provisioner must attach them explicitly. Recorded so the next house provider
doesn't ship the same gap.

## 5. Token endpoint request/response — assumed (unprobed)

Cannot be probed read-only (requires an authenticated code exchange). Assumed
standard OAuth 2.0: `POST application/x-www-form-urlencoded` with
`grant_type=authorization_code`, `code`, `redirect_uri`, `code_verifier`, client
auth via `client_secret_basic`; JSON response carrying `access_token`,
`id_token`, `token_type`, `expires_in`. The implementation consumes only
`id_token`; no refresh token is requested or stored (data-model.md). Promotes
to captured the first time a real login round trip is observed in dev.

## 6. ID-token validation requirements (design hard rules, grounded in §1–§2)

1. Signature verified against the JWKS at the configured `jwks_uri`; keys cached
   in memory, refetched on unknown `kid`. (The current house key is shared
   across providers — §2 — so a `kid` collision across issuers is possible;
   `iss` validation below, not the key, is what binds the token to hireling.)
2. **Algorithm pinned to RS256** — the fixture advertises RS256 only; pinning
   forecloses the alg-confusion class.
3. `iss` string-equal to `https://auth.flinntech.com/application/o/hireling/`.
4. `aud` equal to the configured client id.
5. `exp` unexpired (small clock-skew leeway).
6. `nonce` equal to the value in the transaction cookie set at
   `/api/auth/login`.

## 7. Claim mapping (spec Assumption)

| Claim | Maps to |
|---|---|
| `sub` | `accounts.sub` — allowlist key, session binding, all ownership bindings |
| `preferred_username` | `accounts.username` |
| `name` (fallback `preferred_username`) | `accounts.display_name` |

## 8. Provisioning status

Done (2026-09-23, Josh-delegated): provider pk 5 `hireling` (confidential,
`sub_mode=user_uuid`, house signing key, strict redirect URI), application
"Hireling" slug `hireling` with launch_url, client id/secret stashed at
`op://vex-lab/authentik-mimir`.

Remaining:

1. **Users Becky and Jake have no Authentik accounts yet** (Josh owes their
   emails). Bear, Dave, Bruce, and flinn exist.
2. Once all six exist: copy the six user UUIDs from the Authentik directory
   into `HIRELING_ALLOWLIST` (and Bruce's into `HIRELING_GM_SUB`).
