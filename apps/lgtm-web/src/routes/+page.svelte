<script lang="ts">
  import { getReviewDemo, reviewDemo } from '$lib/services/review'

  let name = $state<string>(reviewDemo.defaultName)
  let greeting = $state('')
  let error = $state('')
  let pending = $state(false)

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    pending = true
    greeting = ''
    error = ''
    try {
      const review = await getReviewDemo(name, `${window.location.origin}/rpc`)
      greeting = review.message
    } catch {
      error = 'Could not reach the backend. Start LGTM and try again.'
    } finally {
      pending = false
    }
  }
</script>

<svelte:head>
  <title>{reviewDemo.title}</title>
  <meta name="description" content={reviewDemo.description} />
</svelte:head>

<main>
  <h1>{reviewDemo.title}</h1>
  <p>{reviewDemo.description}</p>
  <form onsubmit={submit}>
    <label for="name">Your name</label>
    <input id="name" bind:value={name} />
    <button type="submit" disabled={pending}>{pending ? 'Sending…' : 'Say hello'}</button>
  </form>
  <p aria-live="polite">{greeting}</p>
  {#if error}
    <p role="alert">{error}</p>
  {/if}
</main>
