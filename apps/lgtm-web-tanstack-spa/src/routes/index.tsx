import { createFileRoute } from '@tanstack/react-router'

import { ReviewLayout } from '../components/ReviewLayout'
import '../styles/review.css'

export const Route = createFileRoute('/')({
  component: Home,
})

function Home() {
  // Config comes from URL params so the same build drives different repos.
  // Defaults point at the PoC fixture.
  const search = typeof window === 'undefined' ? new URLSearchParams() : new URLSearchParams(window.location.search)
  const config = {
    baseUrl: search.get('backend') ?? 'http://127.0.0.1:50051',
    repoPath: search.get('repo') ?? defaultRepoPath(),
    baseRef: search.get('base') ?? 'HEAD~5',
  }
  return <ReviewLayout config={config} />
}

function defaultRepoPath(): string {
  if (typeof window === 'undefined') return ''
  // Convenience default: current URL path isn't the repo path. Expect caller
  // to pass ?repo=/abs/path. Fallback is the ripgrep fixture under the PoC
  // repo on this dev machine (only useful locally).
  return (
    (window as unknown as { __LGTM_DEFAULT_REPO__?: string }).__LGTM_DEFAULT_REPO__ ??
    '/Users/siddharthkapoor/.herdr/worktrees/lgtm/poc-gpui-rpc-sqlite/.tmp/repos/ripgrep'
  )
}
