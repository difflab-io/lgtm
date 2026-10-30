import { createClient } from '@connectrpc/connect'
import { createConnectTransport } from '@connectrpc/connect-web'
import { ReviewService } from './gen/lgtm/v1/review_pb'

export async function helloReview(
  name: string,
  baseUrl: string,
  fetchImplementation: typeof fetch = fetch,
): Promise<string> {
  const client = createClient(
    ReviewService,
    createConnectTransport({ baseUrl, fetch: fetchImplementation }),
  )
  const reply = await client.helloReview({ name }, { timeoutMs: 5000 })
  return reply.message
}
