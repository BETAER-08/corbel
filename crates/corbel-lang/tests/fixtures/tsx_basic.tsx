import React from "react";
import { Child } from "./Child";

export function Comp({ name }: { name: string }): JSX.Element {
    return (
        <div className="wrap">
            <Child name={name} />
            <Namespace.Item />
            <span>{name}</span>
        </div>
    );
}

export const Arrow = (): JSX.Element => {
    return <Comp name="x" />;
};

function plain() {
    return helper();
}

function helper() {
    return 1;
}

export function caller() {
    plain();
    return <Comp name="y" />;
}
