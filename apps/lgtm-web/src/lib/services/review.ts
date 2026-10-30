import { helloReview } from '../rpc'

/** Display fixtures for the skeleton UI, independent of backend storage. */
export const reviewDemo = {
  title: 'LGTM Hello Review',
  description: 'A demo of the shared review API; reviews are not saved yet.',
  defaultName: 'World',
} as const

export async function getReviewDemo(
  name: string,
  baseUrl: string,
  fetchImplementation: typeof fetch = fetch,
): Promise<{ demo: true; message: string }> {
  return { demo: true, message: await helloReview(name, baseUrl, fetchImplementation) }
}
