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

export class LgtmClient {
  constructor(private readonly baseUrl: string) {}

  getChangedFiles(req: GetChangedFilesRequest): Promise<GetChangedFilesResponse> {
    return this.unary('GetChangedFiles', req)
  }

  getFileDiff(req: GetFileDiffRequest): Promise<GetFileDiffResponse> {
    return this.unary('GetFileDiff', req)
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
      // fetch() throws on network failure (backend not running, CORS, DNS…)
      // Give the user something they can act on instead of the raw TypeError.
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
