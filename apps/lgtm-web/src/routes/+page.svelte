<script lang="ts">
  import { sayHello } from '$lib/rpc'

  let name = $state('World')
  let greeting = $state('')
  let error = $state('')
  let pending = $state(false)

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    pending = true
    greeting = ''
    error = ''
    try {
      greeting = await sayHello(name, `${window.location.origin}/rpc`)
    } catch {
      error = 'Could not reach the backend. Check that lgtm-backend is running.'
    } finally {
      pending = false
    }
  }
</script>

<svelte:head>
  <title>LGTM Hello World</title>
  <meta name="description" content="Say hello through the LGTM backend" />
</svelte:head>

<main>
  <h1>LGTM Hello World</h1>
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
