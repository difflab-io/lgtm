// Minimal identity model for the PoC SPA.
//
// The chosen author lives in localStorage so each reviewer identifies as
// themselves without a login flow. First launch picks a sensible default
// (name = "you", kind = user) that can be edited from the sidebar.

import type { AuthorKind, LgtmClient, User } from './lgtm-client'

const STORAGE_KEY = 'lgtm.identity.v1'

export interface Identity {
  name: string
  kind: AuthorKind
}

export function loadIdentity(): Identity {
  if (typeof window === 'undefined') return defaults()
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (!raw) return defaults()
    const parsed = JSON.parse(raw) as Partial<Identity>
    if (!parsed.name) return defaults()
    return {
      name: parsed.name,
      kind: parsed.kind ?? 'AUTHOR_KIND_USER',
    }
  } catch {
    return defaults()
  }
}

export function saveIdentity(id: Identity) {
  if (typeof window === 'undefined') return
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(id))
}

function defaults(): Identity {
  return { name: 'you', kind: 'AUTHOR_KIND_USER' }
}

/**
 * Resolve the current identity to a User on the server, upserting if needed.
 * The returned User id is required by Create/AppendComment.
 */
export async function ensureUser(client: LgtmClient, id: Identity): Promise<User> {
  const res = await client.upsertUser({ name: id.name, kind: id.kind })
  return res.user
}
