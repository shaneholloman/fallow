<script lang="ts">
import FancyButton from './FancyButton.svelte';
import { isActive } from './utils';
import { tooltip } from './utils';
import { myAttach, Counter } from './utils';
import * as utils from './utils';
import { unusedImported } from './utils';

let name = 'world';
const counter = new Counter();
</script>

<FancyButton />
<div {@attach myAttach}>Attached</div>
<button use:tooltip class:active={isActive}>Hover me</button>
<button onclick={() => counter.bump()}>{counter.value}</button>
<h1>Hello {utils.formatName(name)}!</h1>
