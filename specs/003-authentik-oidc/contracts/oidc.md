# Contract: House Authentik OIDC (E3)

**Status**: instance shape **captured** (verbatim fixture below, probed read-only
2026-09-23); hireling application URLs **assumed-until-probed** (slug
substitutions of the captured pattern — the `hireling` application does not exist
yet).

Probing discipline: read-only `GET`s only. The house rule against writing to
Authentik was honored; no token-endpoint exchange was attempted (impossible
without client credentials anyway), so the token response shape is **assumed**.

## 1. Captured fixture — application `chat` (verbatim)

`GET https://auth.flinntech.com/application/o/chat/.well-known/openid-configuration`
→ 200, captured 2026-09-23:

```json
{
    "issuer": "https://auth.flinntech.com/application/o/chat/",
    "authorization_endpoint": "https://auth.flinntech.com/application/o/authorize/",
    "token_endpoint": "https://auth.flinntech.com/application/o/token/",
    "userinfo_endpoint": "https://auth.flinntech.com/application/o/userinfo/",
    "end_session_endpoint": "https://auth.flinntech.com/application/o/chat/end-session/",
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
    "jwks_uri": "https://auth.flinntech.com/application/o/chat/jwks/",
    "grant_types_supported": [
        "authorization_code",
        "refresh_token",
        "implicit",
        "client_credentials",
        "password",
        "urn:ietf:params:oauth2:grant-type:device_code"
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

`GET https://auth.flinntech.com/application/o/chat/jwks/` → 200, captured
2026-09-23 (single signing key, public key material):

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

Supporting observations (read-only):

- `GET https://auth.flinntech.com/.well-known/openid-configuration` → **404**:
  no root issuer; a per-application slug is mandatory.
- `GET https://auth.flinntech.com/application/o/hireling/.well-known/openid-configuration`
  → **404**: the hireling application is not yet provisioned.
- `GET https://auth.flinntech.com/` → 302 to
  `/flows/-/default/authentication/?next=/` (instance is live and serving).
- Instance version: Authentik **2026.5.6** (from the JWKS `x5c` certificate CN).

## 2. Instance-shape facts the design relies on (captured)

| Fact | Value (from fixture) |
|---|---|
| Issuer pattern | `https://auth.flinntech.com/application/o/<slug>/` — per-application |
| Authorization endpoint | `https://auth.flinntech.com/application/o/authorize/` — **shared across applications** |
| Token endpoint | `https://auth.flinntech.com/application/o/token/` — shared |
| Token endpoint auth | `client_secret_basic`, `client_secret_post` → confidential client; design uses `client_secret_basic` |
| JWKS URI pattern | `https://auth.flinntech.com/application/o/<slug>/jwks/` — per-application |
| ID token signing | **RS256 only** |
| PKCE | `plain` and `S256`; design uses `S256` |
| Scopes | `openid`, `email`, `profile` |
| Claims | `sub`, `iss`, `aud`, `exp`, `iat`, `auth_time`, `acr`, `amr`, `nonce`, `email`, `email_verified`, `name`, `given_name`, `preferred_username`, `nickname`, `groups` |
| Subject type | `public` |
| End-session | `https://auth.flinntech.com/application/o/<slug>/end-session/` — exists but **unused**: logout is local-only (design review decision; the house IdP session is shared with other apps) |

## 3. Hireling application contract — `assumed-until-probed`

Every URL below is the captured pattern with slug `hireling` substituted. None
has been observed live; each promotes to **captured** when the application exists
and the discovery document is re-fetched.

| Item | Assumed value |
|---|---|
| Issuer (`HIRELING_OIDC_ISSUER`) | `https://auth.flinntech.com/application/o/hireling/` |
| Authorization endpoint | `https://auth.flinntech.com/application/o/authorize/` (shared — this one IS captured) |
| Token endpoint | `https://auth.flinntech.com/application/o/token/` (shared — captured) |
| JWKS URI | `https://auth.flinntech.com/application/o/hireling/jwks/` |
| Redirect URI to register | `{HIRELING_BASE_URL}/api/auth/callback` (prod: `https://hireling.flinntech.com/api/auth/callback`) |

**Token endpoint request/response shape — assumed** (not probed; requires client
credentials): standard OAuth 2.0 — `POST application/x-www-form-urlencoded` with
`grant_type=authorization_code`, `code`, `redirect_uri`, `code_verifier`, client
auth via `client_secret_basic`; JSON response carrying `access_token`,
`id_token`, `token_type`, `expires_in`. The implementation consumes only
`id_token`; no refresh token is requested or stored (data-model.md).

**ID-token validation requirements** (the design's hard rules, grounded in the
captured fixture):

1. Signature verified against the JWKS at the configured `jwks_uri`; keys cached
   in memory, refetched on unknown `kid`.
2. **Algorithm pinned to RS256** — the fixture advertises RS256 only; pinning
   forecloses the alg-confusion class.
3. `iss` string-equal to the configured issuer.
4. `aud` equal to the configured client id.
5. `exp` unexpired (small clock-skew leeway).
6. `nonce` equal to the value in the transaction cookie set at `/api/auth/login`.

**Claim mapping** (spec Assumption):

| Claim | Maps to |
|---|---|
| `sub` | `accounts.sub` — allowlist key, session binding, all ownership bindings |
| `preferred_username` | `accounts.username` |
| `name` (fallback `preferred_username`) | `accounts.display_name` |

## 4. What Josh must supply to promote assumed → captured

1. **Provision the Authentik provider + application** with slug `hireling`
   (authorization-code flow, confidential client).
2. **Provider sub mode: "Based on User's UUID"** — so `sub` is the stable user
   UUID visible in the Authentik directory (this is what makes the sub-keyed
   allowlist writable at provisioning time: the six UUIDs are copied from the
   directory into `HIRELING_ALLOWLIST`).
3. **Register the redirect URI** `https://hireling.flinntech.com/api/auth/callback`
   (plus the dev origin if local login against the real IdP is ever wanted).
4. **Provision the six users** (Josh, Bear, Dave, Becky, Jake, Bruce) — already
   his operational task per spec.
5. **Stash the client id + secret in 1Password**; they reach the app as
   `HIRELING_OIDC_CLIENT_ID` / `HIRELING_OIDC_CLIENT_SECRET` via deploy env.
   The secret never enters the repo.
6. Tell the E3 implementer it's done → re-probe
   `…/application/o/hireling/.well-known/openid-configuration`, replace §3's
   assumed values with the captured document, and drop the
   `assumed-until-probed` marker.
