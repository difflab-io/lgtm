import { createClient } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import { GreeterService } from './gen/lgtm/v1/hello_pb'

export async function sayHello(
  name: string,
  baseUrl: string,
  fetchImplementation: typeof fetch = fetch,
): Promise<string> {
  const client = createClient(
    GreeterService,
    createGrpcWebTransport({ baseUrl, fetch: fetchImplementation }),
  )
  const reply = await client.sayHello({ name }, { timeoutMs: 5000 })
  return reply.message
}
