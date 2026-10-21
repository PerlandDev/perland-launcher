import { useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../../ipc/commands";
import type { Instance } from "../../ipc/schema";
import { qk } from "../../state/queries";
import { navigate } from "../../state/route";
import { useRun } from "../../state/runStore";
import { pushError } from "../../state/toasts";
import {
  Copy,
  Folder,
  Gear,
  Image,
  Link,
  Pencil,
  Play,
  Reload,
  Share,
  Trash,
  Undo,
} from "../../ui/Icon";
import { copyText } from "../../ui/copyText";
import type { MenuItem } from "../../ui/Menu";
import { ManageServerLink } from "../servers/ManageServerLink";
import { ShareDialog } from "../share/ShareDialog";
import { ImageDialog } from "./ImageDialog";
import { GoBackDialog } from "./GoBackDialog";
import { DeleteDialog, RenameDialog } from "./InstanceDialogs";
import { addressText, inviteOf, isPack, isServer } from "./labels";
import { useRestorePoint } from "./useRestorePoint";

export interface InstanceActions {
  /** Every action, in menu order: the ⋯ key, a right-click and the rail all show this one list. */
  items: MenuItem[];
  /** The two Home also shows as keys of their own, so a newcomer finds them without opening the menu. */
  settings: MenuItem;
  image: MenuItem;
  /** The dialogs the items open; render them once beside whatever shows the items. */
  dialogs: ReactNode;
}

/** The one definition of what can be done to an instance. Add an action here and every place that offers them gets it. */
export function useInstanceActions(instance: Instance): InstanceActions {
  const { t } = useTranslation();
  const [dialog, setDialog] = useState<
    "rename" | "image" | "delete" | "manage" | "goBack" | "share" | null
  >(null);
  const qc = useQueryClient();
  const close = () => setDialog(null);
  const invite = inviteOf(instance);
  const address = instance.server?.address ?? null;
  // A run saves the whole instance when it syncs, so linking waits until it is idle.
  const running = useRun(instance.id).state !== "idle";
  const serverItems: MenuItem[] = isServer(instance)
    ? [
        {
          id: "playAndJoin",
          label: t("server.playAndJoin"),
          icon: <Play />,
          onSelect: () =>
            void commands.play(instance.id, true).catch(pushError),
        },
        {
          id: "manageLink",
          label: t("server.manage.menu"),
          icon: <Link />,
          onSelect: () => setDialog("manage"),
        },
        ...(invite
          ? [
              {
                id: "copyLink",
                label: t("server.copyLink"),
                icon: <Copy />,
                onSelect: () => void copyText(invite),
              },
            ]
          : []),
        ...(address
          ? [
              {
                id: "copyAddress",
                label: t("server.copyAddress"),
                icon: <Copy />,
                onSelect: () => void copyText(addressText(address)),
              },
            ]
          : []),
      ]
    : [];
  // Vanilla and imported instances can become server instances; packs keep their own source.
  const linkItems: MenuItem[] = isPack(instance)
    ? []
    : [
        {
          id: "linkToServer",
          label: t("server.linkToServer"),
          icon: <Link />,
          disabled: running,
          onSelect: () =>
            navigate({ name: "joinServer", invite: "", linkFor: instance.id }),
        },
      ];
  const point = useRestorePoint(instance.id).data ?? null;
  const held = instance.updateHold !== undefined;
  // Going back and updating again take the instance's lock, so while it runs they are gone, not greyed.
  const restoreItems: MenuItem[] = running
    ? []
    : [
        ...(held
          ? [
              {
                id: "updateAgain",
                label: t("restore.updateAgain"),
                icon: <Reload />,
                onSelect: () =>
                  void commands
                    .packUpdateAgain(instance.id)
                    .then(() =>
                      qc.invalidateQueries({ queryKey: qk.instances }),
                    )
                    .catch(pushError),
              },
            ]
          : []),
        ...(point?.kind === "beforeChange"
          ? [
              {
                id: "goBack",
                label: t("restore.menu"),
                icon: <Undo />,
                onSelect: () => setDialog("goBack"),
              },
            ]
          : []),
        // On a held pack, Update again is the way forward; undoing the go-back there would drop the hold.
        ...(point?.kind === "beforeGoingBack" && !held
          ? [
              {
                id: "undoGoBack",
                label: t("restore.undoMenu"),
                icon: <Undo />,
                onSelect: () => setDialog("goBack"),
              },
            ]
          : []),
      ];
  const settings: MenuItem = {
    id: "settings",
    label: t("home.instanceSettings"),
    icon: <Gear />,
    onSelect: () => navigate({ name: "instanceSettings", id: instance.id }),
  };
  // The core refuses a new picture while a run could save the whole instance over it (R3).
  const image: MenuItem = {
    id: "image",
    label: t("image.menu"),
    icon: <Image />,
    disabled: running,
    onSelect: () => setDialog("image"),
  };
  return {
    items: [
      ...serverItems,
      ...linkItems,
      ...(isPack(instance) && !held
        ? [
            {
              id: "checkUpdates",
              label: t("home.checkUpdates"),
              icon: <Reload />,
              onSelect: () =>
                void qc.invalidateQueries({
                  queryKey: qk.packStatus(instance.id),
                }),
            },
          ]
        : []),
      ...restoreItems,
      settings,
      {
        id: "rename",
        label: t("common.rename"),
        icon: <Pencil />,
        onSelect: () => setDialog("rename"),
      },
      image,
      // A server instance is shared by its invite link, which Copy invite link already offers.
      ...(isServer(instance)
        ? []
        : [
            {
              id: "share",
              label: t("share.menu"),
              icon: <Share />,
              disabled: running,
              onSelect: () => setDialog("share"),
            },
          ]),
      {
        id: "openFolder",
        label: t("common.openFolder"),
        icon: <Folder />,
        onSelect: () =>
          void commands
            .openFolder({ kind: "instance", id: instance.id })
            .catch(pushError),
      },
      {
        id: "delete",
        label: t("common.delete"),
        icon: <Trash />,
        danger: true,
        onSelect: () => setDialog("delete"),
      },
    ],
    settings,
    image,
    dialogs: (
      <>
        {dialog === "rename" && (
          <RenameDialog instance={instance} onClose={close} />
        )}
        {dialog === "image" && (
          <ImageDialog instance={instance} onClose={close} />
        )}
        {dialog === "delete" && (
          <DeleteDialog instance={instance} onClose={close} />
        )}
        {dialog === "manage" && (
          <ManageServerLink instance={instance} onClose={close} />
        )}
        {dialog === "goBack" && point && (
          <GoBackDialog instance={instance} point={point} onClose={close} />
        )}
        {dialog === "share" && (
          <ShareDialog instance={instance} onClose={close} />
        )}
      </>
    ),
  };
}
