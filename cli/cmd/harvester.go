package cmd

import (
	"time"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/harvesters"
	"github.com/util6/assetiweave/internal/output"
)

func newCmdHarvester(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "harvester", Short: "Manage local web conversation harvesters"}
	cmd.AddCommand(newCmdHarvesterTemplate(f))
	cmd.AddCommand(newCmdHarvesterInstall(f))
	cmd.AddCommand(newCmdHarvesterUpdate(f))
	cmd.AddCommand(newCmdHarvesterList(f))
	cmd.AddCommand(newCmdHarvesterRun(f))
	return cmd
}

func newCmdHarvesterTemplate(f *cmdutil.Factory) *cobra.Command {
	cmd := &cobra.Command{Use: "template", Short: "Inspect packaged harvester templates"}
	cmd.AddCommand(newCmdHarvesterTemplateList(f))
	return cmd
}

func newCmdHarvesterTemplateList(f *cmdutil.Factory) *cobra.Command {
	var root string
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List packaged official harvester templates",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			templates, err := harvesters.ListOfficialTemplates(root)
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, map[string]any{"templates": templates})
			return nil
		},
	}
	cmd.Flags().StringVar(&root, "root", "", "harvester root; defaults to ~/.assetiweave/harvesters")
	return cmd
}

func newCmdHarvesterInstall(f *cmdutil.Factory) *cobra.Command {
	var root string
	var source string
	var force bool
	cmd := &cobra.Command{
		Use:   "install <template-id>",
		Short: "Install a packaged or external harvester into ~/.assetiweave",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			var result harvesters.InstallResult
			var err error
			if source != "" {
				result, err = harvesters.InstallPackage(harvesters.InstallPackageOptions{
					Root:   root,
					ID:     args[0],
					Source: source,
					Force:  force,
				})
			} else {
				result, err = harvesters.InstallOfficialTemplate(harvesters.InstallOptions{
					Root:  root,
					ID:    args[0],
					Force: force,
				})
			}
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, result)
			return nil
		},
	}
	cmd.Flags().StringVar(&root, "root", "", "harvester root; defaults to ~/.assetiweave/harvesters")
	cmd.Flags().StringVar(&source, "from", "", "external package directory, archive, or URL")
	cmd.Flags().BoolVar(&force, "force", false, "replace an existing installed template")
	return cmd
}

func newCmdHarvesterUpdate(f *cmdutil.Factory) *cobra.Command {
	var root string
	var source string
	var all bool
	cmd := &cobra.Command{
		Use:   "update [template-id]",
		Short: "Update installed official harvester templates",
		Args: func(cmd *cobra.Command, args []string) error {
			if all {
				return cobra.NoArgs(cmd, args)
			}
			return cobra.ExactArgs(1)(cmd, args)
		},
		RunE: func(cmd *cobra.Command, args []string) error {
			if all && source != "" {
				return errs.NewValidationError(errs.SubtypeInvalidArgument, "--all cannot be used with --from").
					WithCode("validation").
					WithHint("update one external package at a time, for example `harvester update tencent-yuanbao-web --from <url>`")
			}
			var ids []string
			if all {
				templates, err := harvesters.ListOfficialTemplates(root)
				if err != nil {
					return err
				}
				for _, template := range templates {
					ids = append(ids, template.ID)
				}
			} else {
				ids = []string{args[0]}
			}
			results := make([]harvesters.InstallResult, 0, len(ids))
			for _, id := range ids {
				var result harvesters.InstallResult
				var err error
				if source != "" {
					result, err = harvesters.InstallPackage(harvesters.InstallPackageOptions{
						Root:          root,
						ID:            id,
						Source:        source,
						Force:         true,
						PreserveState: true,
					})
				} else {
					result, err = harvesters.InstallOfficialTemplate(harvesters.InstallOptions{
						Root:          root,
						ID:            id,
						Force:         true,
						PreserveState: true,
					})
				}
				if err != nil {
					return err
				}
				results = append(results, result)
			}
			output.WriteSuccess(f.IOStreams.Out, map[string]any{"updated": results})
			return nil
		},
	}
	cmd.Flags().StringVar(&root, "root", "", "harvester root; defaults to ~/.assetiweave/harvesters")
	cmd.Flags().StringVar(&source, "from", "", "external package directory, archive, or URL")
	cmd.Flags().BoolVar(&all, "all", false, "update all packaged official templates")
	return cmd
}

func newCmdHarvesterList(f *cmdutil.Factory) *cobra.Command {
	var root string
	cmd := &cobra.Command{
		Use:   "list",
		Short: "List installed local harvesters",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			installed, err := harvesters.ListInstalled(root)
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, map[string]any{"harvesters": installed})
			return nil
		},
	}
	cmd.Flags().StringVar(&root, "root", "", "harvester root; defaults to ~/.assetiweave/harvesters")
	return cmd
}

func newCmdHarvesterRun(f *cmdutil.Factory) *cobra.Command {
	var root string
	var timeoutSeconds int
	var yes bool
	cmd := &cobra.Command{
		Use:   "run <harvester-id> [args...]",
		Short: "Run an installed harvester script",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := requireYes(yes, "harvester run"); err != nil {
				return err
			}
			result, err := harvesters.Run(harvesters.RunOptions{
				Root:    root,
				ID:      args[0],
				Timeout: time.Duration(timeoutSeconds) * time.Second,
				Args:    args[1:],
			})
			if err != nil {
				return err
			}
			output.WriteSuccess(f.IOStreams.Out, result)
			return nil
		},
	}
	cmd.Flags().StringVar(&root, "root", "", "harvester root; defaults to ~/.assetiweave/harvesters")
	cmd.Flags().IntVar(&timeoutSeconds, "timeout", 600, "maximum run time in seconds")
	cmd.Flags().BoolVar(&yes, "yes", false, "confirm executing this local harvester script")
	return cmd
}
