const { getPlatformPackage } = require('./platform-package');

const pkg = (() => {
  if (process.platform !== 'linux') {
    return getPlatformPackage(process.platform, process.arch);
  }

  try {
    const { familySync } = require('detect-libc');
    return getPlatformPackage(process.platform, process.arch, familySync());
  } catch {
    return getPlatformPackage(process.platform, process.arch);
  }
})();

if (!pkg) {
  console.warn(
    `fallow: No prebuilt binary for ${process.platform}-${process.arch}. ` +
    `You can build from source: https://github.com/fallow-rs/fallow`
  );
  process.exit(0);
}

try {
  require.resolve(pkg);
} catch {
  console.warn(
    `fallow: Platform package ${pkg} not installed. ` +
    `This may happen if you used --no-optional. ` +
    `Run 'npm install' to fix.`
  );
}
