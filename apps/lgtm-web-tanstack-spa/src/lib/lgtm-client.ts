// Hand-written client matching libs/lgtm-grpc/proto/lgtm.proto.
// Connect-JSON unary: POST to `/{package}.{Service}/{Method}`, JSON body.

export interface ChangedFile {
  path: string
  status: string
}
export interface GetChangedFilesRequest {
  repo_path: string
  base_ref: string
}
export interface GetChangedFilesResponse {
  files: ChangedFile[]
}

export interface GetFileDiffRequest {
  repo_path: string
  path: string
  base_ref: string
}
export interface GetFileDiffResponse {
  diff_text: string
}

export type AuthorKind = 'AUTHOR_KIND_USER' | 'AUTHOR_KIND_AGENT' | 'AUTHOR_KIND_UNSPECIFIED'

export interface User {
  id: number
  name: string
  kind: AuthorKind
  created_at: number
}
export interface UpsertUserRequest {
  name: string
  kind: AuthorKind
}
export interface UpsertUserResponse {
  user: User
}

export interface Comment {
  id: number
  thread_id: number
  author: User
  body: string
  created_at: number
}

export interface Thread {
  id: number
  repo_path: string
  base_ref: string
  file_path: string
  line: number
  resolved: boolean
  created_at: number
  comments: Comment[]
}

export interface ListThreadsRequest {
  repo_path: string
  base_ref: string
  file_path?: string
  only_unresolved?: boolean
}
export interface ListThreadsResponse {
  threads: Thread[]
}

export interface CreateThreadRequest {
  repo_path: string
  base_ref: string
  file_path: string
  line: number
  author_id: number
  body: string
}
export interface CreateThreadResponse {
  thread: Thread
}

export interface AppendCommentRequest {
  thread_id: number
  author_id: number
  body: string
}
export interface AppendCommentResponse {
  comment: Comment
}

export interface ResolveThreadRequest {
  thread_id: number
}
export interface ResolveThreadResponse {
  thread: Thread
}

export class LgtmClient {
  constructor(private readonly baseUrl: string) {}

  getChangedFiles(req: GetChangedFilesRequest) {
    return this.unary<GetChangedFilesRequest, GetChangedFilesResponse>('GetChangedFiles', req)
  }
  getFileDiff(req: GetFileDiffRequest) {
    return this.unary<GetFileDiffRequest, GetFileDiffResponse>('GetFileDiff', req)
  }
  upsertUser(req: UpsertUserRequest) {
    return this.unary<UpsertUserRequest, UpsertUserResponse>('UpsertUser', req)
  }
  listThreads(req: ListThreadsRequest) {
    return this.unary<ListThreadsRequest, ListThreadsResponse>('ListThreads', req)
  }
  createThread(req: CreateThreadRequest) {
    return this.unary<CreateThreadRequest, CreateThreadResponse>('CreateThread', req)
  }
  appendComment(req: AppendCommentRequest) {
    return this.unary<AppendCommentRequest, AppendCommentResponse>('AppendComment', req)
  }
  resolveThread(req: ResolveThreadRequest) {
    return this.unary<ResolveThreadRequest, ResolveThreadResponse>('ResolveThread', req)
  }

  private async unary<Req, Res>(method: string, req: Req): Promise<Res> {
    let res: Response
    try {
      res = await fetch(`${this.baseUrl}/lgtm.v1.LgtmService/${method}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(req),
      })
    } catch (e) {
      throw new Error(
        `could not reach lgtm backend at ${this.baseUrl}. ` +
          `Start it with "mise run lgtm:serve" or "mise run lgtm:dev:web". ` +
          `(${(e as Error).message})`,
      )
    }
    if (!res.ok) {
      const body = (await res.json().catch(() => ({}))) as { message?: string }
      throw new Error(body.message ?? `lgtm ${method} failed: ${res.status}`)
    }
    return (await res.json()) as Res
  }
}
