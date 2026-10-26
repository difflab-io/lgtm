/** Select editor coverage explicitly; ordinary runs use only the required profile. */
export const profiles = [
  ...new Set(
    (process.env.LGTM_E2E_NVIM_PROFILES ?? 'required').split(',').map((profile) => profile.trim()),
  ),
].map((profile) => {
  if (profile !== 'required' && profile !== 'kak')
    throw new Error('LGTM_E2E_NVIM_PROFILES must contain required, kak, or both (comma-separated)')
  return profile
})
