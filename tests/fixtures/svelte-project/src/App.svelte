<script lang="ts">
import * as utils from './utils';
import { unusedImported } from './utils';

let name = 'world';
</script>

<h1>Hello {utils.formatName(name)}!</h1>
