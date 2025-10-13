const ngLinkerPlugin = {
    name: 'ng-linker-esbuild',
    setup(build: any) {
        const fs = require('fs');
        const path = require('path');

        build.onLoad({ filter: /\.[cm]?js$/ }, async (args: any) => {
            const source = fs.readFileSync(args.path, 'utf-8');

            // Skip any files that don't have any linker metadata.
            if (!source.includes('ɵɵ')) {
                return undefined;
            }

            const linkerPlugin = require('@angular/compiler-cli/linker/babel').createEs2015LinkerPlugin({
                linkerJitMode: true,
                sourceMapping: false,
                logger: console,
                fileSystem: {
                    resolve: (...paths: string[]) => path.resolve(...paths),
                    exists: (filePath: string) => fs.existsSync(filePath),
                    readFile: (filePath: string) => fs.readFileSync(filePath, 'utf-8'),
                    dirname: (filePath: string) => path.dirname(filePath),
                },
            });

            const babelResult = await require('@babel/core').transformAsync(source, {
                filename: args.path,
                filenameRelative: args.path,
                plugins: [linkerPlugin],
                sourceMaps: 'inline',
            });

            return {
                contents: babelResult?.code || source,
                loader: 'js',
            };
        });
    },
};

// Export the configuration object directly (not as default export)
module.exports = {
    // Ensure only [m]js is consumed. Any typescript should be precompiled
    // and not consumed by esbuild.
    resolveExtensions: ['.mjs', '.js'],
    plugins: [ngLinkerPlugin],
};
