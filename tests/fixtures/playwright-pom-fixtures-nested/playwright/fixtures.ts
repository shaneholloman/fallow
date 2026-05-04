import { test as base } from '@playwright/test';
import { AdminPage } from '../src/pages/admin-page';
import { UserPage } from '../src/pages/user-page';

type MyFixtures = {
  pages: {
    adminPage: AdminPage;
    userPage: UserPage;
  };
};

export { expect } from '@playwright/test';

export const test = base.extend<MyFixtures>({
  pages: async ({}, use) => {
    await use({
      adminPage: new AdminPage(),
      userPage: new UserPage(),
    });
  },
});
