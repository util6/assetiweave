package internalplatform

import (
	"strings"
	"sync"

	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdpolicy"
	"github.com/util6/assetiweave/internal/hook"
)

type Inventory struct {
	Plugins []PluginEntry `json:"plugins"`
}

type PluginEntry struct {
	Name         string           `json:"name"`
	Version      string           `json:"version"`
	Capabilities CapabilitiesView `json:"capabilities"`
	ConfigKeys   []string         `json:"config_keys"`
	Rules        []RuleView       `json:"rules"`
	Observers    []HookEntry      `json:"observers"`
	Wrappers     []HookEntry      `json:"wrappers"`
	Lifecycles   []HookEntry      `json:"lifecycles"`
}

type CapabilitiesView struct {
	RequiredCLIVersion string `json:"required_cli_version,omitempty"`
	Restricts          bool   `json:"restricts"`
	FailurePolicy      string `json:"failure_policy"`
}

type RuleView struct {
	Name             string   `json:"name"`
	Description      string   `json:"description,omitempty"`
	Allow            []string `json:"allow"`
	Deny             []string `json:"deny"`
	MaxRisk          string   `json:"max_risk"`
	Identities       []string `json:"identities"`
	AllowUnannotated bool     `json:"allow_unannotated"`
}

type HookEntry struct {
	Name  string `json:"name"`
	When  string `json:"when,omitempty"`
	Event string `json:"event,omitempty"`
}

func BuildInventory(plugins []PluginInfo, registry *hook.Registry, rules []cmdpolicy.PluginRule) *Inventory {
	inventory := &Inventory{Plugins: make([]PluginEntry, len(plugins))}
	byName := map[string]*PluginEntry{}
	for index, plugin := range plugins {
		inventory.Plugins[index] = PluginEntry{
			Name:         plugin.Name,
			Version:      plugin.Version,
			Capabilities: capabilitiesView(plugin.Capabilities),
			ConfigKeys:   append([]string(nil), plugin.ConfigKeys...),
		}
	}
	for index := range inventory.Plugins {
		byName[inventory.Plugins[index].Name] = &inventory.Plugins[index]
	}
	if registry != nil {
		for _, observer := range registry.Observers() {
			if owner := byName[ownerOf(observer.Name)]; owner != nil {
				owner.Observers = append(owner.Observers, HookEntry{Name: observer.Name, When: whenLabel(observer.When)})
			}
		}
		for _, wrapper := range registry.Wrappers() {
			if owner := byName[ownerOf(wrapper.Name)]; owner != nil {
				owner.Wrappers = append(owner.Wrappers, HookEntry{Name: wrapper.Name})
			}
		}
		for _, lifecycle := range registry.Lifecycles() {
			if owner := byName[ownerOf(lifecycle.Name)]; owner != nil {
				owner.Lifecycles = append(owner.Lifecycles, HookEntry{Name: lifecycle.Name, Event: eventLabel(lifecycle.Event)})
			}
		}
	}
	for _, contribution := range rules {
		if contribution.Rule == nil {
			continue
		}
		if owner := byName[contribution.PluginName]; owner != nil {
			owner.Rules = append(owner.Rules, ruleView(contribution.Rule))
		}
	}
	return inventory
}

func capabilitiesView(capabilities platform.Capabilities) CapabilitiesView {
	return CapabilitiesView{
		RequiredCLIVersion: capabilities.RequiredCLIVersion,
		Restricts:          capabilities.Restricts,
		FailurePolicy:      failurePolicyLabel(capabilities.FailurePolicy),
	}
}

func failurePolicyLabel(policy platform.FailurePolicy) string {
	switch policy {
	case platform.FailOpen:
		return "FailOpen"
	case platform.FailClosed:
		return "FailClosed"
	default:
		return ""
	}
}

func ruleView(rule *platform.Rule) RuleView {
	identities := make([]string, len(rule.Identities))
	for index, identity := range rule.Identities {
		identities[index] = identity.String()
	}
	return RuleView{
		Name:             rule.Name,
		Description:      rule.Description,
		Allow:            append([]string(nil), rule.Allow...),
		Deny:             append([]string(nil), rule.Deny...),
		MaxRisk:          rule.MaxRisk.String(),
		Identities:       identities,
		AllowUnannotated: rule.AllowUnannotated,
	}
}

func ownerOf(name string) string {
	owner, _, ok := strings.Cut(name, ".")
	if !ok {
		return name
	}
	return owner
}

func whenLabel(when platform.When) string {
	switch when {
	case platform.Before:
		return "Before"
	case platform.After:
		return "After"
	default:
		return ""
	}
}

func eventLabel(event platform.LifecycleEvent) string {
	switch event {
	case platform.Startup:
		return "Startup"
	case platform.Shutdown:
		return "Shutdown"
	default:
		return ""
	}
}

var (
	inventoryMu     sync.RWMutex
	activeInventory *Inventory
)

func SetActiveInventory(inventory *Inventory) {
	inventoryMu.Lock()
	defer inventoryMu.Unlock()
	activeInventory = cloneInventory(inventory)
}

func GetActiveInventory() *Inventory {
	inventoryMu.RLock()
	defer inventoryMu.RUnlock()
	return cloneInventory(activeInventory)
}

func cloneInventory(inventory *Inventory) *Inventory {
	if inventory == nil {
		return nil
	}
	cloned := &Inventory{Plugins: make([]PluginEntry, len(inventory.Plugins))}
	for index, plugin := range inventory.Plugins {
		entry := plugin
		entry.ConfigKeys = append([]string(nil), plugin.ConfigKeys...)
		entry.Rules = cloneRuleViews(plugin.Rules)
		entry.Observers = append([]HookEntry(nil), plugin.Observers...)
		entry.Wrappers = append([]HookEntry(nil), plugin.Wrappers...)
		entry.Lifecycles = append([]HookEntry(nil), plugin.Lifecycles...)
		cloned.Plugins[index] = entry
	}
	return cloned
}

func cloneRuleViews(rules []RuleView) []RuleView {
	if rules == nil {
		return nil
	}
	cloned := make([]RuleView, len(rules))
	for index, rule := range rules {
		cloned[index] = rule
		cloned[index].Allow = append([]string(nil), rule.Allow...)
		cloned[index].Deny = append([]string(nil), rule.Deny...)
		cloned[index].Identities = append([]string(nil), rule.Identities...)
	}
	return cloned
}
