import React, { lazy } from 'react';

const Foo = React.lazy(() => import('./Foo'));
const Bar = lazy(() => import('./Bar'));
const Baz = import('./Baz');

export { Foo, Bar, Baz };
