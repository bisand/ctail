import { writable, derived } from 'svelte/store';

export const profiles = writable({});
export const profileNames = derived(profiles, $profiles => Object.keys($profiles));
