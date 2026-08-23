import { expect, test } from "@playwright/test";
import {
    create,
    fromBinary,
    toBinary,
} from "../../frontend-astro/node_modules/@bufbuild/protobuf/dist/esm/index.js";
import {
    ApiResponseSchema,
    ApiV1RequestSchema,
    ApiV1ResponseSchema,
    ContinueDailyReviewResponseSchema,
    EmptySchema,
    FlowCardInfoSchema,
    FlowCardPageSchema,
    ReviewActionValue,
    ReviewAndAdvanceResponseSchema,
    TipcardDetailSchema,
} from "../../frontend-astro/src/generated/denpie_pb";

// Authentication itself has dedicated live-server coverage. These mutation
// cases isolate the authenticated UI state and protobuf API so repeated
// contexts do not compete with the server's auth rate limiter.
test.beforeEach(async ({ page }) => {
    await page.route("**/auth/me", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                id: "review-fixture",
                username: "test",
                role: "user",
                display_name: "Review fixture",
                avatar_data: null,
                build_sha: "playwright",
            }),
        });
    });
});

function flowCard(id: bigint, title: string, tipcardType: string) {
    return create(FlowCardInfoSchema, {
        id,
        title,
        topicName: "Astro migration",
        fullContent: `${title} body`,
        tipcardType,
        status: "active",
    });
}

test("review click sends one typed mutation and replaces only its slot", async ({
    page,
}) => {
    const reviewed = flowCard(11n, "Reviewed card", "repeatable_tip");
    const unrelated = flowCard(22n, "Unrelated card", "casual_tip");
    const replacement = create(FlowCardInfoSchema, {
        ...flowCard(33n, "Replacement card", "repeatable_tip"),
        pinned: false,
    });
    let listCalls = 0;
    let reviewCalls = 0;
    let capturedReview:
        | {
              cardId: bigint;
              grade: number;
              action: ReviewActionValue;
              idempotencyKey: string;
          }
        | undefined;

    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            listCalls += 1;
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards: [reviewed, unrelated],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "reviewAndAdvance") {
            reviewCalls += 1;
            capturedReview = {
                cardId: envelope.call.op.value.cardId,
                grade: envelope.call.op.value.grade,
                action: envelope.call.op.value.action,
                idempotencyKey: envelope.idempotencyKey,
            };
            response = create(ApiResponseSchema, {
                result: {
                    case: "reviewAndAdvance",
                    value: create(ReviewAndAdvanceResponseSchema, {
                        reviewedCardId: 11n,
                        nextCard: replacement,
                        pendingCount: 4,
                        dailyComplete: false,
                        refillScheduled: false,
                    }),
                },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");

    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();
    await expect(grid.locator("li")).toHaveCount(2);
    await expect(grid.locator("li").nth(0)).toHaveAttribute(
        "data-testid",
        "flow-slot-11",
    );
    await expect(grid.locator("li").nth(1)).toContainText("Unrelated card");

    await page.getByTestId("review-again-11").click();

    const swipingCard = page.locator('[data-review-swipe="left"]');
    await expect(swipingCard).toBeVisible();
    await expect(swipingCard).toHaveCSS(
        "animation-name",
        "repeatable-review-swipe-left",
    );

    await expect(grid.locator("li").nth(0)).toHaveAttribute(
        "data-testid",
        "flow-slot-33",
    );
    await expect(grid.locator("li").nth(0)).toContainText("Replacement card");
    await expect(
        grid.locator("li").nth(0).locator('[data-repeatable-stack="3"]'),
    ).toBeVisible();
    await expect(grid.locator("li").nth(1)).toHaveAttribute(
        "data-testid",
        "flow-slot-22",
    );
    await expect(grid.locator("li").nth(1)).toContainText("Unrelated card");

    expect(listCalls).toBe(1);
    expect(reviewCalls).toBe(1);
    expect(capturedReview).toEqual({
        cardId: 11n,
        grade: 1,
        action: ReviewActionValue.AGAIN,
        idempotencyKey: expect.stringMatching(/^[0-9a-f]{32}$/),
    });
});

test("repeatable fullscreen stays open across review replacement", async ({
    page,
}) => {
    const reviewed = flowCard(11n, "Reviewed card", "repeatable_tip");
    const replacement = create(FlowCardInfoSchema, {
        ...flowCard(33n, "Replacement card", "repeatable_tip"),
        fullContent: "Replacement card complete content",
        pendingCount: 4n,
    });
    const originalDetail = create(FlowCardInfoSchema, {
        ...reviewed,
        fullContent: "Reviewed card complete content",
    });
    const detailIds: bigint[] = [];

    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards: [reviewed],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "getTipcard") {
            const id = envelope.call.op.value.id;
            detailIds.push(id);
            response = create(ApiResponseSchema, {
                result: {
                    case: "tipcardDetail",
                    value: create(TipcardDetailSchema, {
                        card: id === 33n ? replacement : originalDetail,
                    }),
                },
            });
        } else if (envelope.call.op.case === "reviewAndAdvance") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "reviewAndAdvance",
                    value: create(ReviewAndAdvanceResponseSchema, {
                        reviewedCardId: 11n,
                        nextCard: replacement,
                        pendingCount: 4,
                        dailyComplete: false,
                        refillScheduled: false,
                    }),
                },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");
    await page.getByTestId("detail-open-11").click();
    const fullscreen = page.getByTestId("card-detail-fullscreen");
    await expect(fullscreen).toBeVisible();
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "Reviewed card complete content",
    );

    await fullscreen.getByTestId("review-again-11").click();
    await expect(fullscreen).toBeVisible();
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "Replacement card complete content",
    );
    await expect(fullscreen.getByTestId("review-again-33")).toBeVisible();
    expect(detailIds).toEqual([11n, 33n]);
});

test("repeatable fullscreen stays open through Continue", async ({
    page,
}) => {
    const reviewed = flowCard(11n, "Reviewed card", "repeatable_tip");
    const continued = create(FlowCardInfoSchema, {
        ...flowCard(44n, "Continued card", "repeatable_tip"),
        fullContent: "Continued card complete content",
        pendingCount: 7n,
    });
    const originalDetail = create(FlowCardInfoSchema, {
        ...reviewed,
        fullContent: "Reviewed card complete content",
    });

    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards: [reviewed],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "getTipcard") {
            const id = envelope.call.op.value.id;
            response = create(ApiResponseSchema, {
                result: {
                    case: "tipcardDetail",
                    value: create(TipcardDetailSchema, {
                        card: id === 44n ? continued : originalDetail,
                    }),
                },
            });
        } else if (envelope.call.op.case === "reviewAndAdvance") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "reviewAndAdvance",
                    value: create(ReviewAndAdvanceResponseSchema, {
                        reviewedCardId: 11n,
                        pendingCount: 0,
                        dailyComplete: true,
                        refillScheduled: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "continueDailyReview") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "continueDailyReview",
                    value: create(ContinueDailyReviewResponseSchema, {
                        availableCards: 2n,
                        activeCardId: 44n,
                        pendingCount: 7,
                    }),
                },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");
    await page.getByTestId("detail-open-11").click();
    const fullscreen = page.getByTestId("card-detail-fullscreen");
    await expect(fullscreen).toBeVisible();

    await fullscreen.getByTestId("review-again-11").click();
    await expect(fullscreen).toBeVisible();
    await expect(fullscreen.getByTestId("continue-11")).toBeVisible();

    await fullscreen.getByTestId("continue-11").click();
    await expect(fullscreen).toBeVisible();
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "Continued card complete content",
    );
    await expect(fullscreen.getByTestId("review-again-44")).toBeVisible();
});

test("Continue on a completed repeatable slot sends one mutation and one detail read", async ({
    page,
}) => {
    const reviewed = flowCard(11n, "Reviewed card", "repeatable_tip");
    const unrelated = flowCard(22n, "Unrelated card", "casual_tip");
    const continued = create(FlowCardInfoSchema, {
        ...flowCard(44n, "Continued card", "repeatable_tip"),
        pinned: false,
    });
    let listCalls = 0;
    let continueCalls = 0;
    const detailCalls: bigint[] = [];
    let capturedContinue:
        | {
              topics: string[];
              tipcardType: string;
              idempotencyKey: string;
          }
        | undefined;

    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            listCalls += 1;
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards: [reviewed, unrelated],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "reviewAndAdvance") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "reviewAndAdvance",
                    value: create(ReviewAndAdvanceResponseSchema, {
                        reviewedCardId: 11n,
                        pendingCount: 0,
                        dailyComplete: true,
                        refillScheduled: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "continueDailyReview") {
            continueCalls += 1;
            capturedContinue = {
                topics: envelope.call.op.value.topics,
                tipcardType: envelope.call.op.value.tipcardType,
                idempotencyKey: envelope.idempotencyKey,
            };
            response = create(ApiResponseSchema, {
                result: {
                    case: "continueDailyReview",
                    value: create(ContinueDailyReviewResponseSchema, {
                        availableCards: 2n,
                        activeCardId: 44n,
                        pendingCount: 7,
                    }),
                },
            });
        } else if (envelope.call.op.case === "getTipcard") {
            detailCalls.push(envelope.call.op.value.id ?? -1n);
            response = create(ApiResponseSchema, {
                result: {
                    case: "tipcardDetail",
                    value: create(TipcardDetailSchema, { card: continued }),
                },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");

    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();
    await expect(grid.locator("li")).toHaveCount(2);

    // Review the repeatable card into a completed slot (dailyComplete, no
    // next card).
    await page.getByTestId("review-again-11").click();
    const completedLi = grid.locator("li").nth(0);
    await expect(completedLi).toHaveAttribute("data-testid", "flow-slot-11");
    await expect(completedLi.getByTestId("review-completed-11")).toBeVisible();

    await completedLi.getByTestId("continue-11").click();

    // The same first li is replaced by the fetched card; the unrelated second
    // li is unchanged.
    await expect(completedLi).toHaveAttribute("data-testid", "flow-slot-44");
    await expect(completedLi).toContainText("Continued card");
    await expect(
        completedLi.locator('[data-repeatable-stack="3"]'),
    ).toBeVisible();
    await expect(grid.locator("li").nth(1)).toHaveAttribute(
        "data-testid",
        "flow-slot-22",
    );
    await expect(grid.locator("li").nth(1)).toContainText("Unrelated card");

    expect(listCalls).toBe(1);
    expect(continueCalls).toBe(1);
    expect(detailCalls).toEqual([44n]);
    expect(capturedContinue).toEqual({
        topics: ["Astro migration"],
        tipcardType: "repeatable_tip",
        idempotencyKey: expect.stringMatching(/^[0-9a-f]{32}$/),
    });
});

test("slow Continue shows a live elapsed indicator instead of a frozen spinner", async ({
    page,
}) => {
    test.setTimeout(20_000);
    const reviewed = flowCard(11n, "Reviewed card", "repeatable_tip");
    const continued = create(FlowCardInfoSchema, {
        ...flowCard(44n, "Continued card", "repeatable_tip"),
        pinned: false,
    });

    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards: [reviewed],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "reviewAndAdvance") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "reviewAndAdvance",
                    value: create(ReviewAndAdvanceResponseSchema, {
                        reviewedCardId: 11n,
                        pendingCount: 0,
                        dailyComplete: true,
                        refillScheduled: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "continueDailyReview") {
            // Hold the mutation open so the continuing state is observable.
            const { promise, resolve } = Promise.withResolvers<void>();
            setTimeout(resolve, 6500);
            await promise;
            response = create(ApiResponseSchema, {
                result: {
                    case: "continueDailyReview",
                    value: create(ContinueDailyReviewResponseSchema, {
                        availableCards: 2n,
                        activeCardId: 44n,
                        pendingCount: 7,
                    }),
                },
            });
        } else if (envelope.call.op.case === "getTipcard") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "tipcardDetail",
                    value: create(TipcardDetailSchema, { card: continued }),
                },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");
    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();

    await page.getByTestId("review-again-11").click();
    const completedLi = grid.locator("li").nth(0);
    await expect(completedLi.getByTestId("review-completed-11")).toBeVisible();

    await completedLi.getByTestId("continue-11").click();

    // While the mutation is in flight the slot shows the continuing alert
    // with an m:ss elapsed counter that actually ticks.
    const saving = completedLi.getByTestId("continue-saving-11");
    await expect(saving).toBeVisible();
    const status = saving.locator('[role="status"]');
    await expect(status).toContainText(/0:0[56]/, { timeout: 10_000 });

    // The held mutation resolves and the slot completes normally.
    await expect(completedLi).toHaveAttribute("data-testid", "flow-slot-44");
});

test("awaiting refill placeholder is replaced in place by the first poll", async ({
    page,
}) => {
    test.setTimeout(15_000);
    const reviewed = flowCard(11n, "Reviewed card", "repeatable_tip");
    const unrelated = flowCard(22n, "Unrelated card", "casual_tip");
    const refilled = flowCard(55n, "Refilled card", "repeatable_tip");
    let listCalls = 0;
    let reviewCalls = 0;
    // Hold refill-poll responses until the placeholder has been asserted, so
    // the test is deterministic regardless of machine speed.
    let releasePoll: (() => void) | undefined;
    const pollGate = new Promise<void>((resolve) => {
        releasePoll = resolve;
    });
    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            listCalls += 1;
            // First read seeds the page; the first 2-second refill poll returns
            // the active replacement for the exact topic. Poll reads (after the
            // review) wait on the gate.
            if (reviewCalls > 0) await pollGate;
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards:
                            reviewCalls === 0
                                ? [reviewed, unrelated]
                                : [refilled, unrelated],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "reviewAndAdvance") {
            reviewCalls += 1;
            response = create(ApiResponseSchema, {
                result: {
                    case: "reviewAndAdvance",
                    value: create(ReviewAndAdvanceResponseSchema, {
                        reviewedCardId: 11n,
                        pendingCount: 0,
                        dailyComplete: false,
                        refillScheduled: true,
                    }),
                },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");

    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();
    await expect(grid.locator("li")).toHaveCount(2);

    // Review with refillScheduled and no next card: the same li becomes the
    // awaiting-refill placeholder.
    await page.getByTestId("review-again-11").click();
    const slotLi = grid.locator("li").nth(0);
    await expect(slotLi).toHaveAttribute("data-testid", "flow-slot-11");
    await expect(slotLi.getByTestId("review-awaiting-refill-11")).toBeVisible();
    await expect(grid.locator("li").nth(1)).toContainText("Unrelated card");

    releasePoll?.();

    // The bounded poll fires after ~2s and swaps only that li.
    await expect(slotLi).toHaveAttribute("data-testid", "flow-slot-55");
    await expect(slotLi).toContainText("Refilled card");
    await expect(grid.locator("li").nth(1)).toHaveAttribute(
        "data-testid",
        "flow-slot-22",
    );
    await expect(grid.locator("li")).toHaveCount(2);
    // Exactly one extra list read: the single successful refill poll.
    expect(listCalls).toBe(2);
    expect(reviewCalls).toBe(1);
});

test("Pin click sends exactly one pinTipcard mutation and updates only its li after success", async ({
    page,
}) => {
    const pinnable = flowCard(
        9007199254740993n,
        "Pinnable card",
        "repeatable_tip",
    );
    const unrelated = flowCard(22n, "Unrelated card", "casual_tip");
    let listCalls = 0;
    let pinCalls = 0;
    let capturedPin:
        { id: bigint; pinned: boolean; idempotencyKey: string } | undefined;
    // Hold the pin response so the saving state is observable before commit.
    let releasePin: (() => void) | undefined;
    const pinReleased = new Promise<void>((resolve) => {
        releasePin = resolve;
    });

    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            listCalls += 1;
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards: [pinnable, unrelated],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "pinTipcard") {
            pinCalls += 1;
            capturedPin = {
                id: envelope.call.op.value.id,
                pinned: envelope.call.op.value.pinned,
                idempotencyKey: envelope.idempotencyKey,
            };
            await pinReleased;
            response = create(ApiResponseSchema, {
                result: { case: "ok", value: create(EmptySchema, {}) },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");
    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();
    await expect(grid.locator("li")).toHaveCount(2);
    // Before any mutation both slots live in the unpinned grid.
    const pinnableLi = page.getByTestId("flow-slot-9007199254740993");
    await expect(pinnableLi).toBeVisible();
    await expect(pinnableLi).toContainText("Pinnable card");

    const pinButton = page.getByTestId("pin-9007199254740993");
    await expect(pinButton).toHaveAttribute(
        "aria-label",
        "Pin card 9007199254740993",
    );
    await pinButton.click();

    // While the mutation is in flight the exact li shows honest saving state
    // and stays in the unpinned grid because card.pinned has not changed yet.
    await expect(page.getByTestId("pin-saving-9007199254740993")).toBeVisible();
    await expect(grid.getByTestId("flow-slot-9007199254740993")).toBeVisible();
    await expect(
        page.getByTestId("review-again-9007199254740993"),
    ).toBeDisabled();
    await expect(
        page.getByTestId("review-skip-9007199254740993"),
    ).toBeDisabled();

    releasePin?.();
    // Only after the authoritative response does the card move into the
    // dedicated pinned section, keeping its stable flow-slot identity.
    const pinnedGrid = page.getByTestId("flow-pinned-grid");
    await expect(pinnedGrid).toBeVisible();
    await expect(
        pinnedGrid.getByTestId("flow-slot-9007199254740993"),
    ).toBeVisible();
    await expect(
        pinnedGrid
            .getByTestId("flow-slot-9007199254740993")
            .getByTestId("card-pinned-9007199254740993"),
    ).toBeVisible();
    await expect(pinButton).toHaveAttribute(
        "aria-label",
        "Unpin card 9007199254740993",
    );
    await expect(
        page.getByTestId("review-again-9007199254740993"),
    ).toBeEnabled();

    // The unrelated slot keeps its identity and content in the unpinned grid.
    const unrelatedLi = grid.getByTestId("flow-slot-22");
    await expect(unrelatedLi).toBeVisible();
    await expect(unrelatedLi).toContainText("Unrelated card");
    await expect(page.getByTestId("card-pinned-22")).toHaveCount(0);

    expect(listCalls).toBe(1);
    expect(pinCalls).toBe(1);
    expect(capturedPin).toEqual({
        id: 9007199254740993n,
        pinned: true,
        idempotencyKey: expect.stringMatching(/^[0-9a-f]{32}$/),
    });
});

test("skip reasons open on hover and rotate the trigger chevron", async ({
    page,
}) => {
    const reviewed = flowCard(11n, "Reviewed card", "repeatable_tip");
    let reviewCalls = 0;
    let capturedReview:
        | {
              cardId: bigint;
              grade: number;
              action: ReviewActionValue;
          }
        | undefined;

    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        const envelope = fromBinary(ApiV1RequestSchema, bytes);
        let response;

        if (envelope.call.op.case === "listFlowCards") {
            response = create(ApiResponseSchema, {
                result: {
                    case: "flowCardPage",
                    value: create(FlowCardPageSchema, {
                        cards: [reviewed],
                        hasMore: false,
                    }),
                },
            });
        } else if (envelope.call.op.case === "reviewAndAdvance") {
            reviewCalls += 1;
            capturedReview = {
                cardId: envelope.call.op.value.cardId,
                grade: envelope.call.op.value.grade,
                action: envelope.call.op.value.action,
            };
            response = create(ApiResponseSchema, {
                result: {
                    case: "reviewAndAdvance",
                    value: create(ReviewAndAdvanceResponseSchema, {
                        reviewedCardId: 11n,
                        pendingCount: 0,
                        dailyComplete: true,
                        refillScheduled: false,
                    }),
                },
            });
        } else {
            throw new TypeError(
                `unexpected API operation ${String(envelope.call.op.case)}`,
            );
        }

        const body = toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        );
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: Buffer.from(body),
        });
    });

    await page.goto("/flow");
    const skipTrigger = page.getByTestId("review-skip-11");
    const skipChevron = skipTrigger.locator("svg");
    await expect(skipTrigger).toBeVisible();
    await expect(skipTrigger).toHaveAttribute("aria-expanded", "false");
    await expect(skipChevron).toHaveCSS("rotate", "none");
    await expect(page.getByTestId("review-skip-known-11")).toHaveCount(0);

    await skipTrigger.hover();
    await expect(skipTrigger).toHaveAttribute("aria-expanded", "true");
    await expect(page.getByTestId("review-skip-known-11")).toBeVisible();
    await expect(page.getByTestId("review-skip-not-interested-11")).toBeVisible();
    await expect(page.getByTestId("review-skip-too-difficult-11")).toBeVisible();
    await expect(skipChevron).toHaveCSS("rotate", "180deg");

    await page.getByTestId("review-skip-known-11").click();
    await expect.poll(() => reviewCalls).toBe(1);
    expect(capturedReview).toEqual({
        cardId: 11n,
        grade: 5,
        action: ReviewActionValue.SKIP_KNOWN,
    });
});
