package rust_language

import (
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"

	"github.com/bazelbuild/bazel-gazelle/config"
	"github.com/bazelbuild/bazel-gazelle/label"
	"github.com/bazelbuild/bazel-gazelle/language"
	"github.com/bazelbuild/bazel-gazelle/rule"
	pb "github.com/calsign/gazelle_rust/proto"
)

type discoveredModule struct {
	response *pb.RustImportsResponse
	testOnly bool
}

type RuleData struct {
	rule            *rule.Rule
	modules         []discoveredModule
	testedCrate     *rule.Rule
	buildScript     *label.Label
	parentCrateName string
	aliases         map[string]string
}

// A plan is prepared in Configure, before Gazelle visits child directories.
// Ownership records every crate, not just the first crate reaching a source.
type cratePlan struct {
	root     string
	target   *rule.Rule
	existing *rule.Rule
	features []string
	edition  string
	manifest bool
	parent   string
	aliases  map[string]string
	modules  map[string]discoveredModule
}

func getTestCrate(r *rule.Rule, repo, pkg string) string {
	if parsed, err := label.Parse(r.AttrString("crate")); err == nil {
		relative := parsed.Rel(repo, pkg)
		if relative.Relative {
			return relative.Name
		}
	}
	return ""
}

func CloneRule(old *rule.Rule) *rule.Rule {
	cloned := rule.NewRule(old.Kind(), old.Name())
	for _, attr := range old.AttrKeys() {
		if attr != "name" {
			cloned.SetAttr(attr, old.Attr(attr))
		}
	}
	return cloned
}

func (l *rustLang) existingRoot(args language.GenerateArgs, r *rule.Rule) string {
	if root := r.AttrString("crate_root"); root != "" {
		kind := l.GetMappedKindInverse(args.Config, r.Kind())
		if kind != "rust_test" && kind != "cargo_build_script" && filepath.Base(root) != "lib.rs" && filepath.Base(root) != "main.rs" {
			l.Log(args.Config, logErr, args.File, "target %s: library/binary crate roots must be lib.rs or main.rs; move %s and update crate_root", r.Name(), root)
			return ""
		}
		return root
	}
	srcs := r.AttrStrings("srcs")
	if len(srcs) == 1 && strings.HasSuffix(srcs[0], ".rs") {
		if l.GetMappedKindInverse(args.Config, r.Kind()) == "rust_test" || r.Kind() == "cargo_build_script" || filepath.Base(srcs[0]) == "lib.rs" || filepath.Base(srcs[0]) == "main.rs" {
			return srcs[0]
		}
	}
	basename := "lib.rs"
	if l.GetMappedKindInverse(args.Config, r.Kind()) == "rust_binary" || r.Kind() == "cargo_build_script" {
		basename = "main.rs"
	}
	candidates := []string{}
	for _, src := range srcs {
		if filepath.Base(src) == basename {
			candidates = append(candidates, src)
		}
	}
	if len(srcs) == 0 {
		for _, src := range []string{basename, "src/" + basename} {
			if fileExists(src, &args) {
				candidates = append(candidates, src)
			}
		}
	}
	if len(candidates) == 1 {
		return candidates[0]
	}
	l.Log(args.Config, logErr, args.File, "target %s: cannot infer a unique Rust crate root; configure crate_root", r.Name())
	return ""
}

func (l *rustLang) prepareCrates(c *config.Config, rel string, f *rule.File) {
	if l.Plans == nil {
		l.Plans = make(map[string][]*cratePlan)
		l.Owners = make(map[string][]string)
		l.BuildScripts = make(map[string]bool)
	}
	args := language.GenerateArgs{Config: c, Rel: rel, Dir: filepath.Join(c.RepoRoot, rel), File: f}
	cfg := l.GetConfig(c)
	if f != nil {
		for _, directive := range f.Directives {
			if directive.Key == "ignore" {
				return
			}
		}
	}
	existingNames := map[string]*rule.Rule{}
	configuredRoots := map[string]bool{}
	plans := []*cratePlan{}
	if f != nil {
		for _, r := range f.Rules {
			existingNames[r.Name()] = r
			kind := l.GetMappedKindInverse(c, r.Kind())
			if kind == "cargo_build_script" {
				l.BuildScripts[rel+":"+r.Name()] = true
			}
			if !SliceContains(resolvableDefs, kind) || r.AttrString("crate") != "" {
				continue
			}
			root := l.existingRoot(args, r)
			if root == "" {
				continue
			}
			cloned := CloneRule(r)
			cloned.SetKind(kind)
			plans = append(plans, &cratePlan{root: root, target: cloned, existing: r, features: r.AttrStrings("crate_features")})
			configuredRoots[filepath.Clean(root)] = true
		}
	}
	add := func(root, name, kind, crateName string) *cratePlan {
		if configuredRoots[filepath.Clean(root)] {
			return nil
		}
		if !fileExists(root, &args) || l.crossesPackage(args, root) || l.excluded(cfg, filepath.Join(rel, root)) {
			return nil
		}
		if owners := l.Owners[filepath.Join(rel, root)]; len(owners) > 0 {
			return nil
		}
		if _, exists := existingNames[name]; exists {
			l.Owners[filepath.Join(rel, root)] = append(l.Owners[filepath.Join(rel, root)], rel+":"+name)
			l.Log(c, logErr, f, "Rust target name %s collides; configure an explicit crate_root and unique name for %s", name, root)
			return nil
		}
		r := rule.NewRule(kind, name)
		if name != crateName {
			r.SetAttr("crate_name", crateName)
		}
		existingNames[name] = r
		enabled := []string{}
		for feature, on := range cfg.EnabledFeatures {
			if on {
				enabled = append(enabled, feature)
			}
		}
		sort.Strings(enabled)
		plan := &cratePlan{root: root, target: r, features: enabled}
		plans = append(plans, plan)
		configuredRoots[filepath.Clean(root)] = true
		return plan
	}
	if fileExists("Cargo.toml", &args) {
		manifest := l.parseCargoToml(c, "Cargo.toml", &args)
		if manifest != nil {
			features := map[string]bool{}
			if cfg.DefaultFeatures {
				for _, feature := range manifest.DefaultFeatures {
					features[feature] = true
				}
			}
			for _, feature := range append(manifest.DefaultFeatures, manifest.NonDefaultFeatures...) {
				if enabled, ok := cfg.EnabledFeatures[feature]; ok {
					features[feature] = enabled
				}
			}
			enabled := []string{}
			for feature, on := range features {
				if on {
					enabled = append(enabled, feature)
				}
			}
			sort.Strings(enabled)
			aliases := map[string]string{}
			for _, alias := range manifest.DependencyAliases {
				aliases[alias.PackageName] = alias.LocalName
			}
			addCargo := func(info *pb.CargoCrateInfo, kind, suffix string) {
				if info == nil || len(info.Srcs) != 1 {
					return
				}
				if info.ProcMacro {
					kind = "rust_proc_macro"
				}
				if kind != "rust_test" && filepath.Base(info.Srcs[0]) != "lib.rs" && filepath.Base(info.Srcs[0]) != "main.rs" {
					l.Log(c, logErr, f, "Cargo target %s: library/binary crate roots must be lib.rs or main.rs; update path %s", info.Name, info.Srcs[0])
					return
				}
				if !fileExists(info.Srcs[0], &args) {
					l.Log(c, logErr, f, "Cargo target %s: root %s does not exist", info.Name, info.Srcs[0])
					return
				}
				if plan := add(info.Srcs[0], info.Name+suffix, kind, strings.ReplaceAll(info.Name, "-", "_")); plan != nil {
					plan.target.SetAttr("visibility", []string{"//visibility:public"})
				}
			}
			suffix := ""
			if manifest.Library != nil {
				for _, bin := range manifest.Binaries {
					if bin.Name == manifest.Library.Name {
						suffix = "_lib"
					}
				}
			}
			addCargo(manifest.Library, "rust_library", suffix)
			for _, bin := range manifest.Binaries {
				addCargo(bin, "rust_binary", "")
			}
			for _, test := range manifest.Tests {
				addCargo(test, "rust_test", "")
			}
			for _, bench := range manifest.Benches {
				addCargo(bench, "rust_binary", "")
			}
			for _, example := range manifest.Examples {
				addCargo(example, "rust_binary", "")
			}
			if fileExists("build.rs", &args) {
				if plan := add("build.rs", "build_script", "cargo_build_script", "build_script"); plan != nil {
					plan.target.SetAttr("visibility", []string{"//visibility:public"})
				}
			}
			for _, plan := range plans {
				if cfg.ExtractCargoLints && plan.target.Kind() != "cargo_build_script" && plan.target.Attr("lint_config") == nil {
					plan.target.SetAttr("lint_config", ":workspace_lints")
				}
				for _, bench := range manifest.Benches {
					if len(bench.Srcs) == 1 && bench.Srcs[0] == plan.root && plan.target.Attr("tags") == nil {
						plan.target.SetAttr("tags", []string{"bench"})
					}
				}
				for _, example := range manifest.Examples {
					if len(example.Srcs) == 1 && example.Srcs[0] == plan.root && plan.target.Attr("tags") == nil {
						plan.target.SetAttr("tags", []string{"example"})
					}
				}
				plan.manifest = true
				plan.parent = manifest.Name
				plan.aliases = aliases
				plan.edition = manifest.Edition
				if plan.existing == nil {
					plan.features = enabled
				}
			}
		}
	} else {
		name := filepath.Base(args.Dir)
		libRoot, binRoot := "src/lib.rs", "src/main.rs"
		if fileExists("lib.rs", &args) {
			libRoot = "lib.rs"
		}
		if fileExists("main.rs", &args) {
			binRoot = "main.rs"
		}
		hasLib := fileExists(libRoot, &args)
		hasBin := fileExists(binRoot, &args)
		if hasLib {
			libName := name
			if hasBin {
				libName += "_lib"
			}
			add(libRoot, libName, "rust_library", strings.ReplaceAll(name, "-", "_"))
		}
		if hasBin {
			add(binRoot, name, "rust_binary", strings.ReplaceAll(name, "-", "_"))
		}
	}
	// Explicit roots are all traversed independently, even when they share files.
	for _, plan := range plans {
		plan.modules = l.discoverModules(c, []string{plan.root}, plan.features, &args)
		for src := range plan.modules {
			key := filepath.Join(rel, src)
			l.Owners[key] = append(l.Owners[key], rel+":"+plan.target.Name())
		}
	}
	// Cargo supplies its own test targets. Without a manifest, test runners must
	// contain tests or a main function; files already owned by a crate are support.
	if !fileExists("Cargo.toml", &args) {
		candidates, _ := filepath.Glob(filepath.Join(args.Dir, "tests", "*.rs"))
		nested, _ := filepath.Glob(filepath.Join(args.Dir, "tests", "*", "main.rs"))
		candidates = append(candidates, nested...)
		for _, absolute := range candidates {
			root, _ := filepath.Rel(args.Dir, absolute)
			if configuredRoots[root] || l.crossesPackage(args, root) || l.excluded(cfg, filepath.Join(rel, root)) {
				continue
			}
			response := l.parseFile(c, root, nil, &args, nil)
			if response == nil || (!response.GetHints().GetHasTest() && !response.GetHints().GetHasMain()) {
				continue
			}
			name := strings.TrimSuffix(filepath.Base(root), ".rs")
			if name == "main" {
				name = filepath.Base(filepath.Dir(root))
			}
			if plan := add(root, name, "rust_test", strings.ReplaceAll(name, "-", "_")); plan != nil {
				plan.modules = l.discoverModules(c, []string{root}, nil, &args)
			}
		}
		// Discover all candidate graphs first, so filesystem ordering cannot turn
		// a support module into a test root.
		support := map[string]bool{}
		for _, plan := range plans {
			for source := range plan.modules {
				if source != plan.root {
					support[source] = true
				}
			}
		}
		retained := plans[:0]
		for _, plan := range plans {
			if plan.existing == nil && plan.target.Kind() == "rust_test" && support[plan.root] {
				continue
			}
			retained = append(retained, plan)
			for source := range plan.modules {
				key := filepath.Join(rel, source)
				l.Owners[key] = append(l.Owners[key], rel+":"+plan.target.Name())
			}
		}
		plans = retained
	}
	l.Plans[rel] = plans
}

func (l *rustLang) crossesPackage(args language.GenerateArgs, src string) bool {
	for dir := filepath.Dir(src); dir != "." && dir != ""; dir = filepath.Dir(dir) {
		if dir == ".." || filepath.IsAbs(dir) {
			return true
		}
		for _, name := range args.Config.ValidBuildFileNames {
			if fileExists(filepath.Join(dir, name), &args) {
				return true
			}
		}
	}
	return false
}

func (l *rustLang) GenerateRules(args language.GenerateArgs) language.GenerateResult {
	result := language.GenerateResult{}
	unitTests := language.GenerateResult{}
	cfg := l.GetConfig(args.Config)
	if cfg.ExtractCargoLints {
		for _, plan := range l.Plans[args.Rel] {
			if plan.manifest && plan.target.Kind() != "cargo_build_script" {
				lint := rule.NewRule("extract_cargo_lints", "workspace_lints")
				lint.SetAttr("manifest", "Cargo.toml")
				lint.SetAttr("workspace", "//:Cargo.toml")
				result.Gen = append(result.Gen, lint)
				result.Imports = append(result.Imports, RuleData{rule: lint})
				break
			}
		}
	}
	names := map[string]bool{}
	tests := map[string]*rule.Rule{}
	if args.File != nil {
		for _, r := range args.File.Rules {
			names[r.Name()] = true
			if crate := getTestCrate(r, args.Config.RepoName, args.Rel); crate != "" {
				tests[crate] = r
			}
		}
	}
	for _, plan := range l.Plans[args.Rel] {
		names[plan.target.Name()] = true
	}
	for _, plan := range l.Plans[args.Rel] {
		if plan.modules == nil {
			continue
		}
		r := plan.target
		srcs := []string{}
		modules := []discoveredModule{}
		data := map[string]bool{}
		hasTest := false
		if plan.manifest {
			data["Cargo.toml"] = true
		}
		for src, module := range plan.modules {
			srcs = append(srcs, src)
			modules = append(modules, module)
			if module.response != nil {
				hasTest = hasTest || module.response.GetHints().GetHasTest()
				for _, file := range module.response.CompileData {
					data[file] = true
				}
			}
		}
		sort.Strings(srcs)
		r.SetAttr("srcs", srcs)
		if plan.existing == nil || plan.existing.Attr("crate_root") == nil {
			r.SetAttr("crate_root", plan.root)
		}
		for _, file := range r.AttrStrings("compile_data") {
			data[file] = true
		}
		if len(data) > 0 && (r.Attr("compile_data") == nil || r.AttrStrings("compile_data") != nil) {
			r.SetAttr("compile_data", setToSortedVector(data))
		}
		if plan.edition != "" && plan.edition != cfg.DefaultEdition && r.Attr("edition") == nil {
			r.SetAttr("edition", plan.edition)
		}
		if len(plan.features) > 0 {
			r.SetAttr("crate_features", plan.features)
		}
		metadata := RuleData{rule: r, modules: modules, parentCrateName: plan.parent, aliases: plan.aliases}
		if plan.manifest && r.Kind() != "cargo_build_script" {
			for _, candidate := range l.Plans[args.Rel] {
				if candidate.target.Kind() == "cargo_build_script" && candidate.root == "build.rs" {
					buildScript := label.New("", args.Rel, candidate.target.Name())
					metadata.buildScript = &buildScript
					break
				}
			}
		}
		result.Gen = append(result.Gen, r)
		result.Imports = append(result.Imports, metadata)
		if r.Kind() != "rust_library" && r.Kind() != "rust_binary" && r.Kind() != "rust_proc_macro" {
			continue
		}
		existingTest := tests[r.Name()]
		if !hasTest && existingTest == nil {
			continue
		}
		var test *rule.Rule
		if existingTest != nil {
			test = CloneRule(existingTest)
		} else {
			name := r.Name() + "_test"
			if names[name] {
				l.Log(args.Config, logErr, args.File, "Rust unit-test name %s collides; configure a rust_test with crate = %q", name, ":"+r.Name())
				continue
			}
			names[name] = true
			test = rule.NewRule("rust_test", name)
			test.SetAttr("crate", ":"+r.Name())
		}
		if plan.manifest && existingTest == nil {
			test.SetAttr("compile_data", []string{"Cargo.toml"})
			if plan.edition != "" && plan.edition != cfg.DefaultEdition {
				test.SetAttr("edition", plan.edition)
			}
			if len(plan.features) > 0 {
				test.SetAttr("crate_features", plan.features)
			}
			if cfg.ExtractCargoLints {
				test.SetAttr("lint_config", ":workspace_lints")
			}
		}
		unitTests.Gen = append(unitTests.Gen, test)
		unitTests.Imports = append(unitTests.Imports, RuleData{rule: test, modules: modules, testedCrate: r, parentCrateName: plan.parent, aliases: plan.aliases})
	}
	result.Gen = append(result.Gen, unitTests.Gen...)
	result.Imports = append(result.Imports, unitTests.Imports...)
	return result
}

func (l *rustLang) parseFile(c *config.Config, file string, enabledFeatures []string, args *language.GenerateArgs, parentNames []string) *pb.RustImportsResponse {
	request := &pb.RustImportsRequest{
		AbsolutePath:    path.Join(args.Dir, file),
		RelativePath:    file,
		EnabledFeatures: enabledFeatures,
		ParentNames:     parentNames,
	}

	response, err := l.Parser.Parse(request)
	if err != nil {
		l.Log(c, logFatal, file, "failed to parse %s: %v", file, err)
	}
	if !response.Success {
		// TODO: It's debatable whether this should be a warning or a fatal error. Having a warning
		// is probably the least surprising, although it could be frustrating to have a bunch of new
		// gazelle errors if there's a parse error in a library that many things depend on.
		l.Log(c, logWarn, file, "failed to parse %s: %s", file, response.ErrorMsg)
		return nil
	}
	return response
}

// Traverse each crate independently. A file reached outside tests wins over a
// test-only path, but a module failure invalidates the whole crate plan.
func (l *rustLang) discoverModules(c *config.Config, roots []string, features []string,
	args *language.GenerateArgs,
) map[string]discoveredModule {
	type pending struct {
		parentNames []string
		file        string
		root        bool
		testOnly    bool
	}
	queue := []pending{}
	for _, root := range roots {
		queue = append(queue, pending{file: root, root: true})
	}
	modules := map[string]discoveredModule{}
	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]
		current.file = filepath.Clean(current.file)
		if old, ok := modules[current.file]; ok && (!old.testOnly || current.testOnly) {
			continue
		}
		if l.crossesPackage(*args, current.file) {
			l.Log(c, logErr, current.file, "Rust module crosses a nested BUILD boundary; move the BUILD boundary or explicitly export and configure the source")
			return nil
		}
		if !fileExists(current.file, args) {
			l.Log(c, logErr, current.file, "Rust crate root or module does not exist")
			return nil
		}
		key := filepath.Join(args.Rel, current.file)
		l.Owners[key] = append(l.Owners[key], args.Rel+":"+roots[0])
		response := l.parseFile(c, current.file, features, args, current.parentNames)
		if response == nil {
			return nil
		}
		modules[current.file] = discoveredModule{response: response, testOnly: current.testOnly}
		dir := filepath.Dir(current.file)
		moduleDir := dir
		if !current.root && filepath.Base(current.file) != "mod.rs" {
			moduleDir = filepath.Join(dir, strings.TrimSuffix(filepath.Base(current.file), ".rs"))
		}
		for _, declaration := range response.Modules {
			base := moduleDir
			if declaration.InlinePathFromFile {
				base = dir
			}
			for _, component := range declaration.InlinePath {
				base = filepath.Join(base, component)
			}
			candidates := []string{filepath.Join(base, declaration.Name+".rs"), filepath.Join(base, declaration.Name, "mod.rs")}
			if declaration.Path != nil {
				if len(declaration.InlinePath) == 0 {
					base = dir
				}
				candidates = []string{filepath.Join(base, *declaration.Path)}
			}
			found := []string{}
			for _, candidate := range candidates {
				if fileExists(candidate, args) {
					found = append(found, candidate)
				}
			}
			if len(found) != 1 {
				l.Log(c, logErr, current.file, "module %s: expected exactly one source among %v, found %d; configure #[path] or fix the module files", declaration.Name, candidates, len(found))
				return nil
			}
			var parentNames []string
			if len(declaration.InlinePath) == 0 {
				parentNames = response.ProvidedNames
			}
			queue = append(queue, pending{file: found[0], testOnly: current.testOnly || declaration.TestOnly, parentNames: parentNames})
		}
	}
	return modules
}

func (l *rustLang) parseCargoToml(c *config.Config, file string, args *language.GenerateArgs) *pb.CargoTomlResponse {
	request := &pb.CargoTomlRequest{FilePath: path.Join(args.Dir, file)}
	response, err := l.Parser.ParseCargoToml(request)
	if err != nil {
		l.Log(c, logFatal, file, "failed to parse Cargo.toml: %v", err)
	}
	if !response.Success {
		l.Log(c, logWarn, file, "failed to parse Cargo.toml: %s", response.ErrorMsg)
		return nil
	}
	return response
}

func fileExists(path string, args *language.GenerateArgs) bool {
	fullPath := filepath.Join(args.Dir, path)
	_, err := os.Stat(fullPath)
	return err == nil
}

func setToSortedVector(src map[string]bool) []string {
	result := []string{}
	for key := range src {
		result = append(result, key)
	}
	sort.Strings(result)
	return result
}

// Exclusions suppress root discovery. Explicitly configured crates still follow
// their declarations, including source files deliberately excluded from Gazelle.
func (l *rustLang) excluded(cfg *rustConfig, file string) bool {
	for candidate := file; candidate != "." && candidate != ""; candidate = filepath.Dir(candidate) {
		for _, pattern := range cfg.Exclusions {
			if match, _ := path.Match(pattern, candidate); match {
				return true
			}
		}
	}
	return false
}
