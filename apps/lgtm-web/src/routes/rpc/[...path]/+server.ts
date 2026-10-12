import { env } from '$env/dynamic/private'
import type { RequestHandler } from './$types'

// Keep browser calls on the web app's origin; forward gRPC-Web to the Rust server.
export const POST: RequestHandler = async ({ params, request, fetch }) => {
  const backendUrl = env.LGTM_BACKEND_URL ?? 'http://127.0.0.1:8080'
  const url = new URL(backendUrl)
  url.pathname = `/${params.path}`
  const headers = new Headers()
  for (const name of ['content-type', 'x-grpc-web', 'grpc-timeout', 'x-user-agent']) {
    const value = request.headers.get(name)
    if (value !== null) headers.set(name, value)
  }

  try {
    const response = await fetch(url, {
      method: 'POST',
      headers,
      body: await request.arrayBuffer(),
    })
    const responseHeaders = new Headers()
    for (const name of ['content-type', 'grpc-encoding', 'grpc-status', 'grpc-message']) {
      const value = response.headers.get(name)
      if (value !== null) responseHeaders.set(name, value)
    }
    return new Response(response.body, {
      status: response.status,
      headers: responseHeaders,
    })
  } catch {
    return new Response('LGTM backend is unavailable', { status: 502 })
  }
}
