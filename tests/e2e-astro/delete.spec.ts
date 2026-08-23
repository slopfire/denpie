import { expect, test, type Page, type Route } from "@playwright/test";
import {
  create,
  fromBinary,
  toBinary,
} from "../../frontend-astro/node_modules/@bufbuild/protobuf/dist/esm/index.js";
import {
  ApiErrorSchema,
  ApiResponseSchema,
  ApiV1RequestSchema,
  ApiV1ResponseSchema,
  EmptySchema,
  FlowCardInfoSchema,
  FlowCardPageSchema,
  type ApiResponse,
  type ApiV1Request,
} from "../../frontend-astro/src/generated/denpie_pb";

test.beforeEach(async ({ page }) => {
  await page.route("**/auth/me", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        id: "delete-fixture",
        username: "test",
        role: "user",
        display_name: "Delete fixture",
        avatar_data: null,
        build_sha: "playwright",
      }),
    });
  });
});

function flowCard(id: bigint, title: string, pinned = false) {
  return create(FlowCardInfoSchema, {
    id,
    title,
    topicName: "Astro migration",
    fullContent: `${title} body`,
    tipcardType: "repeatable_tip",
    status: "active",
    pinned,
  });
}

function decodeRequest(route: Route): ApiV1Request {
  const bytes = route.request().postDataBuffer();
  if (bytes === null) throw new TypeError("missing protobuf request body");
  return fromBinary(ApiV1RequestSchema, bytes);
}

async function fulfillSuccess(
  route: Route,
  request: ApiV1Request,
  response: ApiResponse,
) {
  await route.fulfill({
    status: 200,
    contentType: "application/x-protobuf",
    body: Buffer.from(
      toBinary(
        ApiV1ResponseSchema,
        create(ApiV1ResponseSchema, {
          requestId: request.requestId,
          outcome: { case: "success", value: response },
        }),
      ),
    ),
  });
}

async function fulfillList(
  route: Route,
  request: ApiV1Request,
  cards: ReturnType<typeof flowCard>[],
) {
  await fulfillSuccess(
    route,
    request,
    create(ApiResponseSchema, {
      result: {
        case: "flowCardPage",
        value: create(FlowCardPageSchema, { cards, hasMore: false }),
      },
    }),
  );
}

async function openDeleteConfirmation(page: Page, id: bigint) {
  await page.getByTestId(`card-more-${id}`).click();
  await page.getByTestId(`delete-card-${id}`).click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await expect(page.getByText("This action cannot be undone.")).toBeVisible();
}

test("confirmed delete sends one exact mutation, disables only that card, and removes it without refetch", async ({
  page,
}) => {
  const deletedId = 9007199254740993n;
  const deleted = flowCard(deletedId, "Delete me", true);
  const unrelated = flowCard(22n, "Keep me");
  let listCalls = 0;
  let deleteCalls = 0;
  let captured: { id: bigint; idempotencyKey: string } | undefined;
  let releaseDelete: (() => void) | undefined;
  const deleteReleased = new Promise<void>((resolve) => {
    releaseDelete = resolve;
  });

  await page.addInitScript((id) => {
    window.localStorage.setItem("denpie-pinned-card-order", `[${id}]`);
  }, deletedId.toString());
  await page.route("**/api/v1", async (route) => {
    const request = decodeRequest(route);
    if (request.call.op.case === "listFlowCards") {
      listCalls += 1;
      await fulfillList(route, request, [deleted, unrelated]);
      return;
    }
    if (request.call.op.case !== "deleteTipcard") {
      throw new TypeError(`unexpected operation ${String(request.call.op.case)}`);
    }
    deleteCalls += 1;
    captured = {
      id: request.call.op.value.id,
      idempotencyKey: request.idempotencyKey,
    };
    await deleteReleased;
    await fulfillSuccess(
      route,
      request,
      create(ApiResponseSchema, {
        result: { case: "ok", value: create(EmptySchema, {}) },
      }),
    );
  });

  await page.goto("/flow");
  await openDeleteConfirmation(page, deletedId);
  await page.getByTestId(`delete-cancel-${deletedId}`).click();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  await expect(page.getByTestId(`flow-slot-${deletedId}`)).toBeVisible();
  expect(deleteCalls).toBe(0);

  await openDeleteConfirmation(page, deletedId);
  await page.getByTestId(`delete-confirm-${deletedId}`).click();

  await expect(page.getByTestId(`delete-saving-${deletedId}`)).toBeVisible();
  await expect(page.getByTestId(`pin-${deletedId}`)).toBeDisabled();
  await expect(page.getByTestId(`card-more-${deletedId}`)).toBeDisabled();
  await expect(page.getByTestId(`review-again-${deletedId}`)).toBeDisabled();
  await expect(page.getByTestId(`pinned-drag-${deletedId}`)).toBeDisabled();
  await expect(page.getByTestId("pin-22")).toBeEnabled();
  await expect(page.getByTestId("review-again-22")).toBeEnabled();

  releaseDelete?.();
  await expect(page.getByTestId(`flow-slot-${deletedId}`)).toHaveCount(0);
  await expect(page.getByTestId("flow-slot-22")).toContainText("Keep me");
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.localStorage.getItem("denpie-pinned-card-order"),
      ),
    )
    .toBe("[]");

  expect(listCalls).toBe(1);
  expect(deleteCalls).toBe(1);
  expect(captured).toEqual({
    id: deletedId,
    idempotencyKey: expect.stringMatching(/^[0-9a-f]{32}$/),
  });
});

test("determinate delete failure stays visible, retries safely, then reaches empty", async ({
  page,
}) => {
  const cardId = 7n;
  const keys: string[] = [];
  let deleteCalls = 0;
  let listCalls = 0;

  await page.route("**/api/v1", async (route) => {
    const request = decodeRequest(route);
    if (request.call.op.case === "listFlowCards") {
      listCalls += 1;
      await fulfillList(route, request, [flowCard(cardId, "Only card")]);
      return;
    }
    if (request.call.op.case !== "deleteTipcard") {
      throw new TypeError(`unexpected operation ${String(request.call.op.case)}`);
    }
    deleteCalls += 1;
    keys.push(request.idempotencyKey);
    if (deleteCalls === 1) {
      await route.fulfill({
        status: 200,
        contentType: "application/x-protobuf",
        body: Buffer.from(
          toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
              requestId: request.requestId,
              outcome: {
                case: "error",
                value: create(ApiErrorSchema, {
                  code: 13,
                  message: "delete rejected",
                  retryable: false,
                }),
              },
            }),
          ),
        ),
      });
      return;
    }
    await fulfillSuccess(
      route,
      request,
      create(ApiResponseSchema, {
        result: { case: "ok", value: create(EmptySchema, {}) },
      }),
    );
  });

  await page.goto("/flow");
  await openDeleteConfirmation(page, cardId);
  await page.getByTestId(`delete-confirm-${cardId}`).click();

  const error = page.getByTestId(`delete-error-${cardId}`);
  await expect(error).toBeVisible();
  await expect(error).toContainText("delete rejected");
  await expect(page.getByTestId(`flow-slot-${cardId}`)).toBeVisible();

  await page.getByTestId(`delete-retry-${cardId}`).click();
  await expect(page.getByTestId("flow-empty")).toBeVisible();
  await expect(page.getByTestId(`flow-slot-${cardId}`)).toHaveCount(0);

  expect(listCalls).toBe(1);
  expect(deleteCalls).toBe(2);
  expect(keys[0]).toMatch(/^[0-9a-f]{32}$/);
  expect(keys[1]).toMatch(/^[0-9a-f]{32}$/);
  expect(keys[1]).not.toBe(keys[0]);
});
