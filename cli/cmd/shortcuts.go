package cmd

import (
	"sort"
	"strings"
	"unicode"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
)

// commandAliases keeps the common command surface compact without changing the
// canonical paths used by the Engine contract, policies, hooks, or scripts.
var commandAliases = map[string][]string{
	"accept":         {"ok"},
	"agent":          {"ag"},
	"acquire":        {"acq"},
	"active":         {"cur"},
	"adapter":        {"ad"},
	"add":            {"a"},
	"api":            {"rpc"},
	"app":            {"ap"},
	"apply":          {"a"},
	"archive":        {"ar"},
	"audit":          {"au"},
	"asset":          {"as"},
	"auth-check":     {"ach"},
	"auth-detect":    {"adt"},
	"backup":         {"b"},
	"block":          {"b"},
	"call":           {"c"},
	"candidate":      {"cand"},
	"cancel":         {"cx"},
	"chat":           {"ch"},
	"confirm":        {"cf"},
	"catalog":        {"cat"},
	"check":          {"chk"},
	"completion":     {"comp"},
	"config":         {"cfg"},
	"conversation":   {"c", "conv"},
	"context":        {"ctx"},
	"data":           {"dt"},
	"create":         {"cr", "new"},
	"delete":         {"rm", "del"},
	"draft":          {"drf"},
	"disable":        {"off"},
	"doctor":         {"doc"},
	"enable":         {"on"},
	"exclusive":      {"ex"},
	"export":         {"ex"},
	"get":            {"g"},
	"group":          {"g"},
	"harvester":      {"hv"},
	"import":         {"imp"},
	"incremental":    {"inc"},
	"index":          {"idx"},
	"inspect":        {"info"},
	"install":        {"in"},
	"installed":      {"ins"},
	"item":           {"it"},
	"list":           {"ls", "l"},
	"leader":         {"ld"},
	"memory":         {"m", "mem"},
	"mailbox":        {"mb"},
	"member":         {"mem"},
	"market":         {"mkt"},
	"members":        {"ms"},
	"merge":          {"mg"},
	"mount":          {"mt"},
	"overview":       {"ov", "o"},
	"package":        {"pkg"},
	"part":           {"pt"},
	"plugins":        {"plg"},
	"preview":        {"pv"},
	"profile":        {"p", "prof"},
	"project":        {"prj"},
	"promote":        {"pro"},
	"question":       {"q"},
	"recall":         {"rec"},
	"rebuild":        {"rb"},
	"read":           {"rd"},
	"recent":         {"rcnt"},
	"refresh":        {"ref"},
	"retry":          {"rt"},
	"register":       {"reg"},
	"reject":         {"rej"},
	"resolve":        {"resv"},
	"review":         {"rv"},
	"reinstall":      {"re"},
	"remote":         {"rem"},
	"remove":         {"rm"},
	"replay":         {"rpl"},
	"repair":         {"fix"},
	"restore":        {"res"},
	"rollback":       {"rb"},
	"run":            {"r"},
	"runtime-status": {"rs"},
	"save":           {"sv"},
	"scan":           {"sc"},
	"schema":         {"sch"},
	"scaffold":       {"init"},
	"script":         {"scr"},
	"search":         {"s", "find"},
	"send":           {"snd"},
	"session":        {"ses"},
	"set":            {"s"},
	"settings":       {"st"},
	"show":           {"sh"},
	"skill":          {"sk"},
	"source":         {"src"},
	"split":          {"sp"},
	"status":         {"st"},
	"stream":         {"str"},
	"switch":         {"sw"},
	"sync":           {"sy"},
	"task":           {"tsk"},
	"tasks":          {"tsks"},
	"template":       {"tpl"},
	"tenant":         {"t", "tn"},
	"team":           {"tm"},
	"tool":           {"tl"},
	"turn":           {"trn"},
	"translation":    {"tr"},
	"try-run":        {"try"},
	"uninstall":      {"un"},
	"unmount":        {"um"},
	"unregister":     {"unreg"},
	"update":         {"up", "u"},
	"upgrade":        {"up"},
	"validate":       {"val"},
	"verify":         {"vf"},
	"version":        {"v", "ver"},
	"web":            {"w"},
	"web-record":     {"wr"},
}

var preferredFlagShorthands = map[string][]string{
	"adapter":             {"a"},
	"ai":                  {"A"},
	"all":                 {"a"},
	"app":                 {"a"},
	"asset":               {"a"},
	"branch":              {"b"},
	"browser":             {"b"},
	"catalog-url":         {"c"},
	"check":               {"c"},
	"check-updates":       {"c"},
	"clear":               {"c"},
	"current-project":     {"c"},
	"directory":           {"D"},
	"dry-run":             {"d"},
	"enabled":             {"e"},
	"engine":              {"E"},
	"force":               {"f"},
	"format":              {"f"},
	"from":                {"f"},
	"full":                {"F"},
	"group":               {"g"},
	"id":                  {"i"},
	"include-unavailable": {"I"},
	"item":                {"i"},
	"json":                {"j"},
	"kind":                {"k"},
	"limit":               {"l"},
	"location":            {"l"},
	"method":              {"m"},
	"mode":                {"m"},
	"name":                {"n"},
	"offset":              {"o"},
	"output-root":         {"o"},
	"path":                {"p"},
	"plugin-config":       {"C"},
	"policy":              {"P"},
	"priority":            {"r"},
	"profile":             {"p"},
	"project":             {"p"},
	"provider":            {"r"},
	"query":               {"q"},
	"root":                {"r"},
	"session":             {"S"},
	"source":              {"s"},
	"status":              {"t"},
	"text":                {"t"},
	"timeout":             {"t"},
	"title":               {"T"},
	"until":               {"u"},
	"url":                 {"u"},
	"version":             {"v"},
	"yes":                 {"y"},
}

func applyCommandShortcuts(root *cobra.Command) {
	applyAliases(root)
	assignFlagShorthands(root, map[string]struct{}{"h": {}})
}

func applyAliases(parent *cobra.Command) {
	children := parent.Commands()
	occupied := make(map[string]struct{}, len(children)*2)
	for _, child := range children {
		occupied[child.Name()] = struct{}{}
		for _, alias := range child.Aliases {
			occupied[alias] = struct{}{}
		}
	}
	for _, child := range children {
		for _, alias := range commandAliases[child.Name()] {
			if _, exists := occupied[alias]; exists {
				continue
			}
			child.Aliases = append(child.Aliases, alias)
			occupied[alias] = struct{}{}
		}
		applyAliases(child)
	}
}

func assignFlagShorthands(command *cobra.Command, inherited map[string]struct{}) {
	used := cloneStringSet(inherited)
	persistent := snapshotFlags(command.PersistentFlags())
	local := snapshotFlags(command.Flags())
	if len(persistent) > 0 || len(local) > 0 {
		// pflag indexes shorthands when AddFlag runs. Re-add the existing Flag
		// values so aliases parse correctly instead of only appearing in help.
		command.ResetFlags()
		assignFlags(persistent, used)
		assignFlags(local, used)
		for _, flag := range persistent {
			command.PersistentFlags().AddFlag(flag)
		}
		for _, flag := range local {
			command.Flags().AddFlag(flag)
		}
	}

	for _, child := range command.Commands() {
		assignFlagShorthands(child, used)
	}
}

func snapshotFlags(flags *pflag.FlagSet) []*pflag.Flag {
	var result []*pflag.Flag
	flags.VisitAll(func(flag *pflag.Flag) {
		result = append(result, flag)
	})
	return result
}

func assignFlags(flags []*pflag.Flag, used map[string]struct{}) {
	for _, flag := range flags {
		if flag.Shorthand != "" {
			used[flag.Shorthand] = struct{}{}
		}
	}

	var pending []*pflag.Flag
	for _, flag := range flags {
		if !flag.Hidden && flag.Shorthand == "" {
			pending = append(pending, flag)
		}
	}
	sort.SliceStable(pending, func(i, j int) bool {
		return flagPriority(pending[i].Name) < flagPriority(pending[j].Name)
	})
	for _, flag := range pending {
		for _, shorthand := range flagShorthandCandidates(flag.Name) {
			if _, exists := used[shorthand]; exists {
				continue
			}
			flag.Shorthand = shorthand
			used[shorthand] = struct{}{}
			break
		}
	}
}

func flagPriority(name string) string {
	if _, preferred := preferredFlagShorthands[name]; preferred {
		return "0/" + name
	}
	return "1/" + name
}

func flagShorthandCandidates(name string) []string {
	result := append([]string(nil), preferredFlagShorthands[name]...)
	parts := strings.Split(name, "-")
	for _, part := range parts {
		if part != "" {
			result = appendUnique(result, string([]rune(part)[0]))
		}
	}
	for _, char := range strings.ReplaceAll(name, "-", "") {
		if unicode.IsLetter(char) || unicode.IsDigit(char) {
			result = appendUnique(result, string(char))
		}
	}
	for _, candidate := range append([]string(nil), result...) {
		runes := []rune(candidate)
		if len(runes) == 1 && unicode.IsLetter(runes[0]) {
			result = appendUnique(result, string(unicode.ToUpper(runes[0])))
		}
	}
	for char := 'a'; char <= 'z'; char++ {
		result = appendUnique(result, string(char))
		result = appendUnique(result, string(unicode.ToUpper(char)))
	}
	for char := '0'; char <= '9'; char++ {
		result = appendUnique(result, string(char))
	}
	return result
}

func appendUnique(values []string, value string) []string {
	if value == "" {
		return values
	}
	for _, existing := range values {
		if existing == value {
			return values
		}
	}
	return append(values, value)
}

func cloneStringSet(input map[string]struct{}) map[string]struct{} {
	output := make(map[string]struct{}, len(input))
	for value := range input {
		output[value] = struct{}{}
	}
	return output
}
