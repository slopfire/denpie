// Pure lab-gallery logic: map checked-in card fixtures onto the production
// `FlowCardView` projection so the lab page renders the same data shape as
// the live Flow. No fetch, no React.
import { create } from "@bufbuild/protobuf";
import {
    CardSourceSchema,
    FlowCardInfoSchema,
    type TipcardImageInfo,
    TipcardImageInfoSchema,
} from "@/generated/denpie_pb";
import { toFlowCardView, type FlowCardView } from "@/lib/flow-view";

export interface LabCardFixtureJson {
    id: string;
    topic_name: string;
    title: string;
    full_content: string;
    compressed_content: string;
    tipcard_type: string;
    status: string;
    pinned: boolean;
    pending_count: number;
    created_at?: string;
    next_review_at?: string;
    repeat_count?: number;
    topic_color?: string;
    topic_icon?: string;
    sources?: readonly {
        document_id: number;
        source_type: string;
        title: string;
        url?: string;
    }[];
    images?: readonly string[];
    review_message?: string | null;
    notes: string;
}

/** Production image attachment message built from a fixture data URL. */
export function fixtureImage(
    index: number,
    downloadPath: string,
): TipcardImageInfo {
    return create(TipcardImageInfoSchema, {
        id: BigInt(index + 1),
        position: BigInt(index),
        mimeType: "image/png",
        byteSize: 0n,
        downloadPath,
    });
}

/** Build the exact protocol message the live Flow would receive. */
export function fixtureToFlowCard(fixture: LabCardFixtureJson, index: number) {
    return create(FlowCardInfoSchema, {
        id: BigInt(index + 1),
        topicName: fixture.topic_name,
        topicIcon: fixture.topic_icon ?? "radix-icons:bookmark",
        topicColor: fixture.topic_color ?? "",
        title: fixture.title,
        fullContent: fixture.full_content,
        compressedContent: fixture.compressed_content,
        createdAt: fixture.created_at ?? "",
        tipcardType: fixture.tipcard_type,
        status: fixture.status,
        nextReviewAt: fixture.next_review_at ?? "",

        repeatCount: fixture.repeat_count ?? 0,
        pinned: fixture.pinned,
        pendingCount: BigInt(fixture.pending_count),
        images: (fixture.images ?? []).map((path, position) =>
            fixtureImage(position, path),
        ),
        sources: (fixture.sources ?? []).map((source) =>
            create(CardSourceSchema, {
                documentId: BigInt(source.document_id),
                sourceType: source.source_type,
                title: source.title,
                url: source.url,
            }),
        ),
    });
}

/** Fixture-side stack layer count matching `repeatableStackLayers`. */
export function repeatableStackLayersFromCount(
    tipcardType: string,
    pendingCount: bigint,
): number {
    if (tipcardType !== "repeatable_tip" || pendingCount <= 0n) return 0;
    return pendingCount >= 3n ? 3 : Number(pendingCount);
}

export function labCardViews(fixtures: readonly LabCardFixtureJson[]): Array<{
    fixtureId: string;
    notes: string;
    reviewMessage: string | null;
    stackLayers: number;
    view: FlowCardView;
}> {
    return fixtures.map((fixture, index) => ({
        fixtureId: fixture.id,
        notes: fixture.notes,
        reviewMessage: fixture.review_message ?? null,
        stackLayers: repeatableStackLayersFromCount(
            fixture.tipcard_type,
            BigInt(fixture.pending_count),
        ),
        view: toFlowCardView(fixtureToFlowCard(fixture, index)),
    }));
}
