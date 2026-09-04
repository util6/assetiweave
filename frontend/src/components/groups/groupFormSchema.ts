import { z } from "zod";
import { assetGroupIconSvgSchema } from "../../schemas/group";
import { isHexColor } from "../../theme/colorValidation";
import type { AssetGroupIconSvg, AssetGroupInput } from "../../types";

export interface GroupFormValues {
  name: string;
  description: string;
  color: string;
  displayIcon: string;
  iconSvg: AssetGroupIconSvg | null;
  enabled: boolean;
}

export const groupFormSchema = z.object({
  name: z.string().trim().min(1, "group.form.error.nameRequired"),
  description: z.string(),
  color: z
    .string()
    .trim()
    .refine((val) => isHexColor(val), {
      message: "group.form.error.colorInvalid",
    }),
  displayIcon: z.string(),
  iconSvg: assetGroupIconSvgSchema.nullable(),
  enabled: z.boolean(),
}) satisfies z.ZodType<GroupFormValues>;

export function groupValuesToInput(values: GroupFormValues): AssetGroupInput {
  const trimmedName = values.name.trim();
  const trimmedDescription = values.description.trim();
  const trimmedDisplayIcon = values.displayIcon.trim();
  const trimmedColor = values.color.trim();

  return {
    name: trimmedName,
    description: trimmedDescription.length > 0 ? trimmedDescription : null,
    color: trimmedColor.length > 0 ? trimmedColor : null,
    display_icon: trimmedDisplayIcon.length > 0 ? trimmedDisplayIcon : null,
    icon_svg: values.iconSvg,
    enabled: values.enabled,
  };
}

export function parseGroupIconSvgInput(
  input: string,
): AssetGroupIconSvg | null {
  const trimmed = input.trim();
  if (!trimmed) {
    return null;
  }

  // 1. 尝试作为 JSON 解析并用既有 assetGroupIconSvgSchema 校验
  try {
    const parsedJson = JSON.parse(trimmed);
    const result = assetGroupIconSvgSchema.safeParse(parsedJson);
    if (result.success) {
      return result.data;
    }
  } catch {
    // 不是合法 JSON，继续尝试 SVG 标签解析
  }

  // 2. 尝试作为 SVG Markup 解析
  if (trimmed.includes("<svg") && typeof DOMParser !== "undefined") {
    try {
      const document = new DOMParser().parseFromString(trimmed, "image/svg+xml");
      if (document.querySelector("parsererror")) {
        return null;
      }
      const svg = document.querySelector("svg");
      if (!svg) {
        return null;
      }

      const paths = Array.from(svg.querySelectorAll("path")).flatMap((path) => {
        const d = path.getAttribute("d")?.trim();
        if (!d) return [];
        const clipRule = normalizeSvgRule(
          path.getAttribute("clip-rule") ?? path.getAttribute("clipRule"),
        );
        const fillRule = normalizeSvgRule(
          path.getAttribute("fill-rule") ?? path.getAttribute("fillRule"),
        );
        return [
          {
            d,
            ...(clipRule ? { clip_rule: clipRule } : {}),
            ...(fillRule ? { fill_rule: fillRule } : {}),
          },
        ];
      });
      if (paths.length === 0) {
        return null;
      }

      const viewBox = svg.getAttribute("viewBox")?.trim();
      const candidate = {
        paths,
        ...(viewBox ? { view_box: viewBox } : {}),
      };

      const validated = assetGroupIconSvgSchema.safeParse(candidate);
      return validated.success ? validated.data : null;
    } catch {
      return null;
    }
  }

  return null;
}

function normalizeSvgRule(value: unknown): "evenodd" | "nonzero" | null {
  return value === "evenodd" || value === "nonzero" ? value : null;
}
